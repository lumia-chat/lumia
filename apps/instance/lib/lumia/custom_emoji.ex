defmodule Lumia.CustomEmoji do
  @moduledoc """
  A server's custom emoji (specs/emoji.md): uploaded, renamed and removed
  by whoever holds `MANAGE_EMOJI`, their pictures made by the instance
  (`Lumia.Media.emoji/1`) and kept as blobs it seals, as avatars are, and
  as many as the server's limit allows: its own number set by the
  operator, or the instance's default.

  Every change is told to every member as `EmojiUpdated`, by a job after
  the commit (`Lumia.Workers.EmojiUpdated`).
  """

  import Ecto.Query

  alias Lumia.Accounts
  alias Lumia.Audit
  alias Lumia.Authz.Permission
  alias Lumia.CustomEmoji.Emoji
  alias Lumia.Repo
  alias Lumia.Servers
  alias Lumia.Servers.Server
  alias Lumia.Tx
  alias Lumia.Workers.EmojiUpdated
  alias Lumia.Workers.ServerUpdated

  @name ~r/\A[a-z0-9_]{2,32}\z/
  # The most a limit can be: what `servers.emoji_limit` holds, and within
  # what `Server.emoji_limit` (uint32) carries.
  @most 2_147_483_647
  @written ~r/\A:[a-z0-9_]{2,32}:\z/

  @doc "The most a server's limit, or the default, can be."
  def most, do: @most

  @doc """
  The limit of a server without a number of its own:
  `LUMIA_SERVER_EMOJI_MAX`, 50 unless set (specs/emoji.md 5). Zero is a
  limit: no uploads.
  """
  def default_limit, do: Application.get_env(:lumia, :emoji, [])[:server_max] || 50

  @doc """
  How many live emoji `server` may have: its own number, or the default.
  Never below zero nor above `most/0`, whatever the setting says, since it
  goes out as an unsigned number on every `Server`.
  """
  def limit(%Server{emoji_limit: nil}), do: within(default_limit())
  def limit(%Server{emoji_limit: limit}), do: within(limit)

  defp within(limit) when is_integer(limit), do: limit |> max(0) |> min(@most)

  @doc """
  A name as kept: lowercased, and 2 to 32 of a-z, 0-9 and `_`, or
  `{:error, {:invalid_argument, "name"}}`.
  """
  def name(name) when is_binary(name) do
    name = String.downcase(name)
    if Regex.match?(@name, name), do: {:ok, name}, else: {:error, {:invalid_argument, "name"}}
  end

  def name(_), do: {:error, {:invalid_argument, "name"}}

  @doc "Whether `text` has the shape of a name between colons, `:name:`."
  def written?(text) when is_binary(text), do: Regex.match?(@written, text)

  @doc """
  The server's emoji removed in the last day, each under the name it had
  when it was removed, oldest removal first: what `EmojiUpdated.removed`
  carries (specs/emoji.md 8).
  """
  def recently_removed(server_id) do
    day_ago = DateTime.add(DateTime.utc_now(), -1, :day)

    Repo.all(
      from e in Emoji,
        where: e.server_id == ^server_id and e.deleted_at > ^day_ago,
        order_by: [e.deleted_at, e.id]
    )
  end

  @doc """
  Whether a live emoji's picture is the blob at `address`: an emoji's
  picture is served only then (specs/emoji.md 7).
  """
  def live_picture?(address) when is_binary(address) do
    path = Accounts.picture_path(%Lumia.Blobs.Blob{address: address})
    Repo.exists?(from e in Emoji, where: e.image_url == ^path and is_nil(e.deleted_at))
  end

  @doc "The server's live emoji, oldest first."
  def list(server_id) do
    Repo.all(
      from e in Emoji,
        where: e.server_id == ^server_id and is_nil(e.deleted_at),
        order_by: [e.inserted_at, e.id]
    )
  end

  @doc "The live emoji of each of `server_ids`, a server's together, oldest first."
  def list_of([]), do: []

  def list_of(server_ids) do
    Repo.all(
      from e in Emoji,
        where: e.server_id in ^server_ids and is_nil(e.deleted_at),
        order_by: [e.server_id, e.inserted_at, e.id]
    )
  end

  @doc """
  The server's live emoji and its limit, for a member of it: `{:ok,
  {emoji, limit}}`, or `{:error, :not_found}` for anyone else.
  """
  def for_member(server_id, account_id) do
    with {:ok, _actor} <- member(server_id, account_id),
         %Server{} = server <- Repo.get(Server, server_id) || {:error, :not_found} do
      {:ok, {list(server_id), limit(server)}}
    end
  end

  @doc """
  The emoji of the server named by each of `ids` (16-byte or text UUIDs),
  removed ones included: a map from id to its row. What reactions and a
  message's refs are checked against.
  """
  def of_server(_server_id, []), do: %{}

  def of_server(server_id, ids) do
    Repo.all(from e in Emoji, where: e.server_id == ^server_id and e.id in ^ids)
    |> Map.new(&{&1.id, &1})
  end

  @doc "The names of the emoji named by `ids`, removed ones included: a map from id to name."
  def names([]), do: %{}

  def names(ids) do
    Repo.all(from e in Emoji, where: e.id in ^ids, select: {e.id, e.name}) |> Map.new()
  end

  @doc """
  A new emoji of the server, named `name`, its picture made from `upload`
  (a picture's bytes, as chosen). `MANAGE_EMOJI`. `{:ok, emoji}`, or
  `{:error, reason}`: `:permission_denied`; `{:invalid_argument, "name"}`;
  `{:already_exists, existing_id}` for a name another live emoji of the
  server has; `{:emoji_limit, limit}` at the server's limit; the picture's
  refusals (`:too_large`, `:unsupported`, `:invalid`, `:rate_limited`);
  `{:failed_precondition, "blobs"}` where the instance keeps no blobs.
  """
  def create(server_id, actor_id, name, upload) when is_binary(upload) do
    with {:ok, name} <- name(name),
         {:ok, _actor} <- may_manage(server_id, actor_id),
         :ok <- has_room(server_id),
         true <- Lumia.Blobs.enabled?() || {:error, {:failed_precondition, "blobs"}},
         {:ok, {media_type, bytes, width, height, animated}} <- Lumia.Media.emoji(upload),
         {:ok, blob} <- Lumia.Blobs.put(bytes, "emoji", media_type, actor_id) do
      keep(blob, %{
        server_id: server_id,
        name: name,
        image_url: Accounts.picture_path(blob),
        animated: animated,
        width: width,
        height: height,
        created_by_account_id: actor_id
      })
    else
      {:error, :unavailable} -> {:error, {:failed_precondition, "blobs"}}
      other -> other
    end
  end

  # The emoji's row for the blob just kept, or the blob gone again.
  defp keep(blob, attrs) do
    case Tx.transact(fn -> insert(attrs.server_id, attrs.created_by_account_id, attrs) end) do
      {:ok, emoji} ->
        {:ok, emoji}

      error ->
        Lumia.Blobs.delete(blob.address)
        error
    end
  end

  # The room is checked again here under the server's row, so two uploads
  # at once cannot both take its last place.
  defp insert(server_id, actor_id, attrs) do
    with {:ok, server} <- Servers.lock(server_id),
         {:ok, _actor} <- may_manage(server_id, actor_id),
         :ok <- room(server),
         :ok <- name_free(server_id, attrs.name, nil),
         {:ok, emoji} <- Repo.insert(struct(Emoji, attrs)),
         {:ok, _} <-
           Audit.record({:server, server_id}, actor_id, "emoji.create",
             target: {:emoji, emoji.id},
             after: %{"name" => emoji.name, "animated" => emoji.animated}
           ),
         {:ok, _} <- EmojiUpdated.schedule(server_id) do
      {:ok, emoji}
    end
  end

  @doc """
  Renames one of the server's live emoji. `MANAGE_EMOJI`. `{:ok, emoji}`,
  or the refusals of `create/4` for a name, and `{:error, :not_found}`.
  """
  def rename(server_id, emoji_id, actor_id, name) do
    with {:ok, name} <- name(name),
         do: Tx.transact(fn -> renamed(server_id, emoji_id, actor_id, name) end)
  end

  defp renamed(server_id, emoji_id, actor_id, name) do
    with {:ok, _server} <- Servers.lock(server_id),
         {:ok, _actor} <- may_manage(server_id, actor_id),
         {:ok, emoji} <- live(server_id, emoji_id),
         :ok <- name_free(server_id, name, emoji.id),
         {:ok, renamed} <- emoji |> Ecto.Changeset.change(name: name) |> Repo.update(),
         {:ok, _} <-
           Audit.record({:server, server_id}, actor_id, "emoji.rename",
             target: {:emoji, emoji.id},
             before: %{"name" => emoji.name},
             after: %{"name" => name}
           ),
         {:ok, _} <- EmojiUpdated.schedule(server_id) do
      {:ok, renamed}
    end
  end

  @doc """
  Removes one of the server's live emoji: its row is kept with its name
  and its picture is deleted, so messages and reactions that used it keep
  its name (specs/emoji.md 7). `MANAGE_EMOJI`. `:ok`, or `{:error,
  :not_found | :permission_denied}`.
  """
  def remove(server_id, emoji_id, actor_id) do
    removed =
      Tx.transact(fn ->
        with {:ok, _server} <- Servers.lock(server_id),
             {:ok, _actor} <- may_manage(server_id, actor_id),
             {:ok, emoji} <- live(server_id, emoji_id),
             {:ok, gone} <-
               emoji
               |> Ecto.Changeset.change(deleted_at: DateTime.utc_now(), image_url: nil)
               |> Repo.update(),
             {:ok, _} <-
               Audit.record({:server, server_id}, actor_id, "emoji.delete",
                 target: {:emoji, emoji.id},
                 before: %{"name" => emoji.name}
               ),
             {:ok, _} <- EmojiUpdated.schedule(server_id) do
          {:ok, {emoji, gone}}
        end
      end)

    with {:ok, {emoji, _gone}} <- removed do
      # The one reference is gone. A store that refuses leaves the blob to
      # the hourly sweep, which removes what no row names.
      if address = Accounts.avatar_address(emoji.image_url),
        do: Lumia.Blobs.delete(address, kind: "emoji")

      :ok
    end
  end

  @doc """
  Sets the server's own limit, the operator's to set (`bin/lumia-admin
  emoji-limit`): a whole number from 0 to `most/0`, or `:default` for the
  instance's default.
  Audited with no actor and `override` set, and told to every member as
  `ServerUpdated`. Lowering it deletes nothing (specs/emoji.md 5).
  `{:ok, server}` or `{:error, :not_found | {:invalid_argument, "limit"}}`.
  """
  def set_limit(server_id, limit)
      when limit == :default or (is_integer(limit) and limit >= 0 and limit <= @most) do
    kept = if limit == :default, do: nil, else: limit

    Tx.transact(fn ->
      with {:ok, server} <- Servers.lock(server_id),
           {:ok, updated} <- server |> Ecto.Changeset.change(emoji_limit: kept) |> Repo.update(),
           {:ok, _} <-
             Audit.record({:server, server_id}, nil, "server.emoji_limit",
               before: %{"emoji_limit" => server.emoji_limit},
               after: %{"emoji_limit" => kept},
               override: true
             ),
           # Inserted as a row, not through Oban, which a release's `eval`
           # does not start: the running nodes pick the job up.
           {:ok, _} <- Repo.insert(ServerUpdated.new(%{"server_id" => server_id}, schedule_in: 1)) do
        {:ok, updated}
      end
    end)
  end

  def set_limit(_server_id, _limit), do: {:error, {:invalid_argument, "limit"}}

  # Cheap, before a picture is made; `room/1` decides under the lock.
  defp has_room(server_id) do
    case Repo.get(Server, server_id) do
      nil -> {:error, :not_found}
      server -> room(server)
    end
  end

  defp room(server) do
    limit = limit(server)

    live =
      Repo.aggregate(
        from(e in Emoji, where: e.server_id == ^server.id and is_nil(e.deleted_at)),
        :count
      )

    if live < limit, do: :ok, else: {:error, {:emoji_limit, limit}}
  end

  defp name_free(server_id, name, except) do
    taken =
      Repo.one(
        from e in Emoji,
          where: e.server_id == ^server_id and e.name == ^name and is_nil(e.deleted_at),
          select: e.id
      )

    if is_nil(taken) or taken == except, do: :ok, else: {:error, {:already_exists, taken}}
  end

  defp live(server_id, emoji_id) do
    with {:ok, id} <- Ecto.UUID.cast(emoji_id),
         %Emoji{} = emoji <-
           Repo.one(
             from e in Emoji,
               where: e.id == ^id and e.server_id == ^server_id and is_nil(e.deleted_at)
           ) do
      {:ok, emoji}
    else
      _ -> {:error, :not_found}
    end
  end

  defp member(server_id, account_id) do
    case Servers.actor(server_id, account_id) do
      {:ok, actor} -> {:ok, actor}
      {:error, _} -> {:error, :not_found}
    end
  end

  defp may_manage(server_id, account_id) do
    with {:ok, actor} <- member(server_id, account_id) do
      if actor.owner? or Permission.has?(actor.server_permissions, :manage_emoji),
        do: {:ok, actor},
        else: {:error, :permission_denied}
    end
  end
end
