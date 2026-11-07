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
  """

  alias Lumia.Authz
  alias Lumia.Authz.Cache
  alias Lumia.Authz.Permission
  alias Lumia.Dms
  alias Lumia.Events.Channel
  alias Lumia.Repo
  alias Lumia.Servers

  @doc """
  The account's access to a channel:

      {:ok, %{channel: channel, server_id: id | nil, acl_version: n, permissions: bits}}
      {:error, :not_found}

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
          member -> resolve_in_server(member, channel)
        end
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
    import Ecto.Query

    ids = channel_ids |> Enum.reject(&is_nil/1) |> Enum.uniq()
    rows = Repo.all(from c in Channel, where: c.id in ^ids and is_nil(c.deleted_at))
    by_id = Map.new(rows, &{&1.id, &1})

    personal =
      for %Channel{server_id: nil, kind: kind, id: id} <- rows,
          Dms.personal_kind?(kind),
          do: id

    participant = Dms.participant_of(personal, account_id)

    members =
      rows
      |> Enum.map(& &1.server_id)
      |> Enum.reject(&is_nil/1)
      |> Enum.uniq()
      |> Map.new(&{&1, Servers.member_for(&1, account_id)})

    Map.new(channel_ids, fn id ->
      access = access_of(by_id[id], participant, members)
      {id, with({:ok, access} <- access, do: check(access, permission))}
    end)
  end

  defp access_of(nil, _participant, _members), do: {:error, :not_found}

  defp access_of(%Channel{server_id: nil, kind: kind} = channel, participant, _members) do
    if not Dms.personal_kind?(kind) or MapSet.member?(participant, channel.id),
      do: {:ok, full_access(channel)},
      else: {:error, :not_found}
  end

  defp access_of(%Channel{server_id: server_id} = channel, _participant, members) do
    case members[server_id] do
      nil -> {:error, :not_found}
      member -> resolve_in_server(member, channel)
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
  The channels in a server the account can view, with their access. The
  membership is read once for the server, not once per channel.
  """
  def viewable_channels(account_id, server_id) do
    import Ecto.Query

    case Servers.member_for(server_id, account_id) do
      nil ->
        []

      member ->
        from(c in Channel,
          where: c.server_id == ^server_id and is_nil(c.deleted_at),
          order_by: c.position
        )
        |> Repo.all()
        |> Enum.flat_map(&viewable_access(member, &1))
    end
  end

  defp viewable_access(member, channel) do
    case resolve_in_server(member, channel) do
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
      permissions: Permission.all()
    }
  end

  defp resolve_in_server(member, channel) do
    case Cache.fetch(channel.id, channel.acl_version) do
      {:ok, snapshot} ->
        {:ok,
         %{
           channel: channel,
           server_id: channel.server_id,
           acl_version: channel.acl_version,
           permissions: Authz.resolve(snapshot, member)
         }}

      # The channel moved on between the row read and the snapshot load:
      # re-read the row and resolve at its new version.
      {:error, :stale} ->
        case Repo.get(Channel, channel.id) do
          %Channel{deleted_at: nil} = fresh -> resolve_in_server(member, fresh)
          _ -> {:error, :not_found}
        end

      {:error, _} = e ->
        e
    end
  end
end
