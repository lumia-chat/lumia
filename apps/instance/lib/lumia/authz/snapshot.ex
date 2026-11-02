defmodule Lumia.Authz.Snapshot do
  @moduledoc """
  The ACL snapshot for one channel at one `acl_version`
  (docs/specs/authorization.md section 4): the server's roles and the
  channel's overwrites. A pure value; `Lumia.Authz.resolve/2` is a pure
  function of it and a member.
  """

  import Ecto.Query

  alias Lumia.Events.Channel
  alias Lumia.Repo
  alias Lumia.Servers.ChannelOverwrite
  alias Lumia.Servers.Role
  alias Lumia.Servers.Server

  defstruct channel_id: nil,
            server_id: nil,
            acl_version: 1,
            owner_account_id: nil,
            everyone_role_id: nil,
            roles: %{},
            overwrites: %{}

  @doc """
  Loads the snapshot for a channel from Postgres. Returns the snapshot at
  the channel's *current* `acl_version`; callers that want a specific
  version go through `Lumia.Authz.Cache`.
  """
  def load(channel_id) do
    query =
      from c in Channel,
        left_join: s in Server,
        on: s.id == c.server_id,
        where: c.id == ^channel_id,
        select: {c.server_id, c.acl_version, s.owner_account_id}

    case Repo.one(query) do
      nil ->
        {:error, :channel_not_found}

      {server_id, acl_version, owner_id} ->
        roles = if server_id, do: load_roles(server_id), else: %{}

        {:ok,
         %__MODULE__{
           channel_id: channel_id,
           server_id: server_id,
           acl_version: acl_version,
           owner_account_id: owner_id,
           everyone_role_id: everyone_id(roles),
           roles:
             Map.new(roles, fn r ->
               {r.id, %{permissions: r.permissions, position: r.position}}
             end),
           overwrites: load_overwrites(channel_id)
         }}
    end
  end

  defp load_roles(server_id) do
    Repo.all(from r in Role, where: r.server_id == ^server_id)
  end

  defp everyone_id(roles) do
    case Enum.find(roles, & &1.is_everyone) do
      nil -> nil
      role -> role.id
    end
  end

  defp load_overwrites(channel_id) do
    from(o in ChannelOverwrite, where: o.channel_id == ^channel_id)
    |> Repo.all()
    |> Map.new(fn o ->
      {{String.to_existing_atom(o.target_kind), o.target_id}, %{allow: o.allow, deny: o.deny}}
    end)
  end
end
