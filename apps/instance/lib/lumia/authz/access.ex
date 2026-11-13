defmodule Lumia.Authz.Access do
  @moduledoc """
  Authoritative access checks shared by the gateway and the HTTP API
  (docs/specs/authorization.md section 5).

  `channel/2` reads the channel row from Postgres, which yields the
  authoritative `acl_version`, and resolves the account's permissions
  against the cached snapshot for that version. A non-member sees
  `:not_found`, never `:permission_denied`, so channel existence is not
  disclosed. A `PERSONAL_*` channel resolves to all permissions for a
  participant and `:not_found` for anyone else (architecture 6.4). A
  server-less text channel (spike-level) resolves to all permissions.

  Channel membership sits on top of permission (architecture 6.8): in a
  server's text channel, someone who has not joined holds none of the
  permissions that write (`@member_writes`), and in a private one, being
  a member (not a guest) is what lets them see it. A server DM is its
  members' alone, with `@dm_permissions`, and `:not_found` for anyone
  else, administrators included.
  """

  import Bitwise
  import Ecto.Query

  alias Lumia.Authz
  alias Lumia.Authz.Cache
  alias Lumia.Authz.Permission
  alias Lumia.Dms
  alias Lumia.Events.Channel
  alias Lumia.Repo
  alias Lumia.Servers
  alias Lumia.Servers.ChannelMember

  # What only someone in a channel may do there.
  @member_writes Permission.to_bits([
                   :send_messages,
                   :send_in_threads,
                   :attach_files,
                   :add_reactions,
                   :start_calls
                 ])

  # What each member of a server DM may do in it.
  @dm_permissions Permission.to_bits([
                    :view_channel,
                    :read_history,
                    :send_messages,
                    :send_in_threads,
                    :attach_files,
                    :add_reactions,
                    :start_calls
                  ])

  @doc """
  The account's access to a channel:

      {:ok, %{channel: channel, server_id: id | nil, acl_version: n, permissions: bits,
              joined?: bool}}
      {:error, :not_found}

  `joined?` is whether the account is in the channel; a personal one's
  participant always is.

  A nil channel id (a malformed id from a client) is `:not_found`.
  """
  def channel(_account_id, nil), do: {:error, :not_found}

  def channel(account_id, channel_id) do
    case Repo.get(Channel, channel_id) do
      nil ->
        {:error, :not_found}

      %Channel{deleted_at: %DateTime{}} ->
        {:error, :not_found}

      %Channel{server_id: nil, kind: kind} = channel ->
        if not Dms.personal_kind?(kind) or Dms.participant?(channel.id, account_id),
          do: {:ok, full_access(channel)},
          else: {:error, :not_found}

      %Channel{server_id: server_id} = channel ->
        case Servers.member_for(server_id, account_id) do
          nil -> {:error, :not_found}
          member -> resolve_in_server(member, channel, joined?(channel.id, account_id))
        end
    end
  end

  @doc """
  Whether the account could see a server's text channel once in it
  (architecture 6.8): whom it may join, be added to or be invited to.
  """
  def enterable?(account_id, channel_id) do
    with %Channel{server_id: server_id, deleted_at: nil} = channel when is_binary(server_id) <-
           Repo.get(Channel, channel_id),
         %{} = member <- Servers.member_for(server_id, account_id),
         {:ok, %{permissions: bits}} <- resolve_in_server(member, channel, true) do
      Permission.has?(bits, :view_channel)
    else
      _ -> false
    end
  end

  @doc "Whether the account is in the channel (architecture 6.8)."
  def joined?(channel_id, account_id) do
    Repo.exists?(
      from m in ChannelMember, where: m.channel_id == ^channel_id and m.account_id == ^account_id
    )
  end

  @doc """
  What `member` (the map `Lumia.Servers.member_for/2` gives) may do in a
  server's `channel` under `snapshot`, given whether they are in it: the
  resolution of architecture 7.2 with channel membership on top
  (architecture 6.8). The one place the rule is written; access checks and
  the viewer diff both come here.
  """
  def permissions(_snapshot, _member, %Channel{kind: "server_dm"}, true), do: @dm_permissions
  def permissions(_snapshot, _member, %Channel{kind: "server_dm"}, false), do: 0

  def permissions(snapshot, member, channel, joined?) do
    bits = Authz.resolve(snapshot, member)

    cond do
      not joined? -> bits &&& bnot(@member_writes)
      channel.private and member.kind == :member -> bits ||| Permission.bit(:view_channel)
      true -> bits
    end
  end

  @doc "Like `channel/2`, but `{:error, :not_found}` unless the account can view it."
  def viewable_channel(account_id, channel_id) do
    with {:ok, access} <- channel(account_id, channel_id) do
      if Permission.has?(access.permissions, :view_channel),
        do: {:ok, access},
        else: {:error, :not_found}
    end
  end

  @doc """
  What a command asks beyond the permission its kind needs: a message that
  mentions everyone needs `MENTION_EVERYONE` (architecture 6.6). `:ok` or
  `{:error, :permission_denied}`.
  """
  def check_command(access, %Lumia.V1.Command{op: {:send_message, op}}) do
    everyone? = Enum.any?(op.mentions, &match?(%Lumia.V1.Mention{target: {:everyone, true}}, &1))

    if everyone? and not Permission.has?(access.permissions, :mention_everyone),
      do: {:error, :permission_denied},
      else: :ok
  end

  def check_command(_access, _command), do: :ok

  @doc "`{:ok, access}` when the account holds `permission` in the channel, else an error."
  def require(account_id, channel_id, permission) do
    with {:ok, access} <- channel(account_id, channel_id), do: check(access, permission)
  end

  @doc """
  `require/3` for many channels at once: a map from each channel id to
  its result. The channel rows come in one query, each server's
  membership is read once, and the personal channels' participation in
  one query, so a subscribe to fifty channels of one server costs two
  reads rather than fifty of each. A nil id maps to `{:error, :not_found}`.
  """
  def require_many(account_id, channel_ids, permission) do
    ids = channel_ids |> Enum.reject(&is_nil/1) |> Enum.uniq()
    rows = Repo.all(from c in Channel, where: c.id in ^ids and is_nil(c.deleted_at))
    by_id = Map.new(rows, &{&1.id, &1})

    personal =
      for %Channel{server_id: nil, kind: kind, id: id} <- rows,
          Dms.personal_kind?(kind),
          do: id

    participant = Dms.participant_of(personal, account_id)

    joined =
      MapSet.new(
        Repo.all(
          from m in ChannelMember,
            where: m.channel_id in ^ids and m.account_id == ^account_id,
            select: m.channel_id
        )
      )

    members =
      rows
      |> Enum.map(& &1.server_id)
      |> Enum.reject(&is_nil/1)
      |> Enum.uniq()
      |> Map.new(&{&1, Servers.member_for(&1, account_id)})

    Map.new(channel_ids, fn id ->
      access = access_of(by_id[id], participant, members, joined)
      {id, with({:ok, access} <- access, do: check(access, permission))}
    end)
  end

  defp access_of(nil, _participant, _members, _joined), do: {:error, :not_found}

  defp access_of(%Channel{server_id: nil, kind: kind} = channel, participant, _members, _joined) do
    if not Dms.personal_kind?(kind) or MapSet.member?(participant, channel.id),
      do: {:ok, full_access(channel)},
      else: {:error, :not_found}
  end

  defp access_of(%Channel{server_id: server_id} = channel, _participant, members, joined) do
    case members[server_id] do
      nil -> {:error, :not_found}
      member -> resolve_in_server(member, channel, MapSet.member?(joined, channel.id))
    end
  end

  # A view the account cannot see is not found; one it can see without
  # the permission is denied.
  defp check(access, permission) do
    cond do
      not Permission.has?(access.permissions, :view_channel) -> {:error, :not_found}
      not Permission.has?(access.permissions, permission) -> {:error, :permission_denied}
      true -> {:ok, access}
    end
  end

  @doc """
  The channels in a server the account can view, with their access, in
  the order the sidebar shows them; with `joined: true` only those it is
  in, its server DMs among them (architecture 6.8). The membership is read
  once for the server, not once per channel.
  """
  def viewable_channels(account_id, server_id, opts \\ []) do
    case Servers.member_for(server_id, account_id) do
      nil ->
        []

      member ->
        joined =
          MapSet.new(
            Repo.all(
              from m in ChannelMember,
                join: c in Channel,
                on: c.id == m.channel_id,
                where: c.server_id == ^server_id and m.account_id == ^account_id,
                select: m.channel_id
            )
          )

        from(c in Channel,
          where: c.server_id == ^server_id and is_nil(c.deleted_at),
          # Channels share a position until someone orders them; the id (UUIDv7) keeps
          # those in the order they were made instead of whatever order the heap has.
          order_by: [c.position, c.id]
        )
        |> Repo.all()
        |> Enum.filter(&(not Keyword.get(opts, :joined, false) or MapSet.member?(joined, &1.id)))
        |> Enum.flat_map(&viewable_access(member, &1, MapSet.member?(joined, &1.id)))
    end
  end

  defp viewable_access(member, channel, joined?) do
    case resolve_in_server(member, channel, joined?) do
      {:ok, %{permissions: bits} = access} ->
        if Permission.has?(bits, :view_channel), do: [access], else: []

      {:error, _} ->
        []
    end
  end

  defp full_access(channel) do
    %{
      channel: channel,
      server_id: nil,
      acl_version: channel.acl_version,
      permissions: Permission.all(),
      joined?: true
    }
  end

  # A block between the two (architecture 6.4) leaves what was said readable
  # and nothing more to be said.
  defp resolve_in_server(member, %Channel{kind: "server_dm"} = channel, true) do
    bits = permissions(nil, member, channel, true)

    bits =
      if Dms.blocked_in?(channel.id, member.account_id),
        do: bits &&& bnot(@member_writes),
        else: bits

    {:ok, server_access(channel, bits, true)}
  end

  defp resolve_in_server(_member, %Channel{kind: "server_dm"}, false), do: {:error, :not_found}

  defp resolve_in_server(member, channel, joined?) do
    case Cache.fetch(channel.id, channel.acl_version) do
      {:ok, snapshot} ->
        {:ok, server_access(channel, permissions(snapshot, member, channel, joined?), joined?)}

      # The channel moved on between the row read and the snapshot load:
      # re-read the row and resolve at its new version.
      {:error, :stale} ->
        case Repo.get(Channel, channel.id) do
          %Channel{deleted_at: nil} = fresh -> resolve_in_server(member, fresh, joined?)
          _ -> {:error, :not_found}
        end

      {:error, _} = e ->
        e
    end
  end

  defp server_access(channel, bits, joined?) do
    %{
      channel: channel,
      server_id: channel.server_id,
      acl_version: channel.acl_version,
      permissions: bits,
      joined?: joined?
    }
  end
end
