defmodule LumiaWeb.ServerController do
  @moduledoc "Servers, their channels, members and roles (docs/protocol.md section 9)."

  use Phoenix.Controller, formats: []

  import Ecto.Query

  alias Lumia.Authz.Access
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.Servers
  alias Lumia.Servers.Member
  alias Lumia.Servers.MemberRole
  alias Lumia.Servers.Role
  alias Lumia.Servers.Server
  alias Lumia.V1
  alias LumiaWeb.ProtoConn

  action_fallback LumiaWeb.FallbackController

  def index(conn, _params) do
    account_id = conn.assigns.current_account_id
    memberships = Repo.all(from m in Member, where: m.account_id == ^account_id)
    ids = Enum.map(memberships, & &1.server_id)
    servers = Repo.all(from s in Server, where: s.id in ^ids)

    ProtoConn.render(conn, %V1.ListServersResponse{
      servers: Enum.map(servers, &Proto.server/1),
      memberships: Enum.map(memberships, &Proto.member(&1, role_ids(&1.server_id, account_id)))
    })
  end

  def create(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.CreateServerRequest),
         {:ok, server} <- Servers.create_server(conn.assigns.current_account_id, req.name) do
      ProtoConn.render(conn, 201, Proto.server(server))
    end
  end

  def show(conn, %{"server_id" => server_id}) do
    with {:ok, server} <- member_server(conn, server_id) do
      ProtoConn.render(conn, Proto.server(server))
    end
  end

  def channels(conn, %{"server_id" => server_id}) do
    with {:ok, _server} <- member_server(conn, server_id) do
      channels =
        conn.assigns.current_account_id
        |> Access.viewable_channels(server_id)
        |> Enum.map(&Proto.channel(&1.channel))

      ProtoConn.render(conn, %V1.ListChannelsResponse{channels: channels})
    end
  end

  def members(conn, %{"server_id" => server_id}) do
    with {:ok, _server} <- member_server(conn, server_id) do
      members =
        Repo.all(from m in Member, where: m.server_id == ^server_id, order_by: m.joined_at)

      roles =
        from(mr in MemberRole,
          where: mr.server_id == ^server_id,
          select: {mr.account_id, mr.role_id}
        )
        |> Repo.all()
        |> Enum.group_by(&elem(&1, 0), &elem(&1, 1))

      ids = Enum.map(members, & &1.account_id)
      accounts = Repo.all(from a in Lumia.Accounts.Account, where: a.id in ^ids)

      ProtoConn.render(conn, %V1.ListMembersResponse{
        members: Enum.map(members, &Proto.member(&1, Map.get(roles, &1.account_id, []))),
        accounts: Enum.map(accounts, &Proto.account/1)
      })
    end
  end

  def update_member(conn, %{"server_id" => server_id, "account_id" => account_id}) do
    actor = conn.assigns.current_account_id

    with {:ok, _server} <- member_server(conn, server_id),
         {:ok, req} <- ProtoConn.decode(conn, V1.UpdateMemberRequest),
         wanted = Enum.map(req.role_ids, &Proto.from_uuid/1),
         current = role_ids(server_id, account_id),
         :ok <- each(wanted -- current, &Servers.assign_role(server_id, account_id, &1, actor)),
         :ok <- each(current -- wanted, &Servers.unassign_role(server_id, account_id, &1, actor)),
         member when not is_nil(member) <-
           Repo.get_by(Member, server_id: server_id, account_id: account_id) do
      ProtoConn.render(conn, Proto.member(member, role_ids(server_id, account_id)))
    else
      nil -> {:error, :not_found}
      error -> error
    end
  end

  def remove_member(conn, %{"server_id" => server_id, "account_id" => account_id}) do
    actor = conn.assigns.current_account_id
    reason = if actor == account_id, do: "left", else: "removed"

    with {:ok, _server} <- member_server(conn, server_id),
         :ok <- Servers.remove_member(server_id, account_id, actor, reason) do
      ProtoConn.no_content(conn)
    end
  end

  def roles(conn, %{"server_id" => server_id}) do
    with {:ok, _server} <- member_server(conn, server_id) do
      roles = Repo.all(from r in Role, where: r.server_id == ^server_id, order_by: r.position)
      ProtoConn.render(conn, %V1.ListRolesResponse{roles: Enum.map(roles, &Proto.role/1)})
    end
  end

  def create_role(conn, %{"server_id" => server_id}) do
    with {:ok, _server} <- member_server(conn, server_id),
         {:ok, req} <- ProtoConn.decode(conn, V1.UpsertRoleRequest),
         %V1.Role{} = role <- req.role || {:error, :invalid_argument},
         {:ok, created} <-
           Servers.create_role(server_id, conn.assigns.current_account_id, role_attrs(role)) do
      ProtoConn.render(conn, 201, Proto.role(created))
    end
  end

  def update_role(conn, %{"server_id" => server_id, "role_id" => role_id}) do
    with {:ok, _server} <- member_server(conn, server_id),
         {:ok, req} <- ProtoConn.decode(conn, V1.UpsertRoleRequest),
         %V1.Role{} = role <- req.role || {:error, :invalid_argument},
         {:ok, updated} <-
           Servers.update_role(
             server_id,
             role_id,
             conn.assigns.current_account_id,
             role_attrs(role)
           ) do
      ProtoConn.render(conn, Proto.role(updated))
    end
  end

  def delete_role(conn, %{"server_id" => server_id, "role_id" => role_id}) do
    with {:ok, _server} <- member_server(conn, server_id),
         :ok <- Servers.delete_role(server_id, role_id, conn.assigns.current_account_id) do
      ProtoConn.no_content(conn)
    end
  end

  def set_overwrite(conn, %{"channel_id" => channel_id}) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.Overwrite),
         {:ok, target} <- overwrite_target(req),
         :ok <-
           Servers.set_overwrite(
             channel_id,
             conn.assigns.current_account_id,
             target,
             Enum.map(req.allow, &Proto.from_permission/1),
             Enum.map(req.deny, &Proto.from_permission/1)
           ) do
      ProtoConn.no_content(conn)
    end
  end

  defp overwrite_target(%V1.Overwrite{target: {:role_id, id}}),
    do: {:ok, {:role, Proto.from_uuid(id)}}

  defp overwrite_target(%V1.Overwrite{target: {:account_id, id}}),
    do: {:ok, {:member, Proto.from_uuid(id)}}

  defp overwrite_target(_), do: {:error, :invalid_argument}

  defp role_attrs(%V1.Role{} = role) do
    %{
      name: role.name,
      position: role.position,
      permissions: Enum.map(role.permissions, &Proto.from_permission/1),
      color: if(role.color == "", do: nil, else: role.color)
    }
  end

  # A server is visible only to its members; anyone else gets NOT_FOUND.
  defp member_server(conn, server_id) do
    with %Server{} = server <- Repo.get(Server, server_id),
         member when not is_nil(member) <-
           Servers.member_for(server_id, conn.assigns.current_account_id) do
      {:ok, server}
    else
      _ -> {:error, :not_found}
    end
  end

  defp role_ids(server_id, account_id) do
    Repo.all(
      from mr in MemberRole,
        where: mr.server_id == ^server_id and mr.account_id == ^account_id,
        select: mr.role_id
    )
  end

  defp each(items, fun) do
    Enum.reduce_while(items, :ok, fn item, :ok ->
      case fun.(item) do
        :ok -> {:cont, :ok}
        error -> {:halt, error}
      end
    end)
  end
end
