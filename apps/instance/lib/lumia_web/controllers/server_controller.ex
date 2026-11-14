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
    else
      {:error, {:invalid_argument, field}} ->
        ProtoConn.render_error(conn, :invalid_argument, %{"field" => field})

      other ->
        other
    end
  end

  def show(conn, %{"server_id" => server_id}) do
    with {:ok, server} <- member_server(conn, server_id) do
      ProtoConn.render(conn, Proto.server(server))
    end
  end

  @doc """
  `PATCH /servers/{server_id}`: the server's name, its retention, whether it
  shows deleted messages, any of them (`Lumia.Servers.update_server/3`, architecture 6.7).
  """
  def update(conn, %{"server_id" => server_id}) do
    with {:ok, _server} <- member_server(conn, server_id),
         {:ok, req} <- ProtoConn.decode(conn, V1.UpdateServerRequest),
         changes =
           Map.reject(Map.take(req, [:name, :retention_days, :show_deleted]), fn {_, v} ->
             is_nil(v)
           end),
         {:ok, _} <-
           set_defaults(server_id, conn.assigns.current_account_id, req.default_channels),
         {:ok, updated} <-
           Servers.update_server(server_id, conn.assigns.current_account_id, changes) do
      ProtoConn.render(conn, Proto.server(updated))
    else
      {:error, {:invalid_argument, field}} ->
        ProtoConn.render_error(conn, :invalid_argument, %{"field" => field})

      other ->
        other
    end
  end

  defp set_defaults(_server_id, _actor_id, nil), do: {:ok, nil}

  defp set_defaults(server_id, actor_id, %V1.ChannelIds{channel_ids: ids}) do
    ids = Enum.map(ids, &Proto.from_uuid/1)

    if Enum.all?(ids, &is_binary/1),
      do: Lumia.Channels.set_defaults(server_id, actor_id, ids),
      else: {:error, {:invalid_argument, "default_channels"}}
  end

  @doc "`PUT /servers/{server_id}/icon`: the body is a picture, as chosen (`Lumia.Servers.set_icon/3`)."
  def put_icon(conn, %{"server_id" => server_id}) do
    with {:ok, _server} <- member_server(conn, server_id),
         {:ok, upload, conn} <- LumiaWeb.Pictures.read(conn),
         {:ok, updated} <- Servers.set_icon(server_id, conn.assigns.current_account_id, upload) do
      ProtoConn.render(conn, Proto.server(updated))
    else
      other -> LumiaWeb.Pictures.refused(conn, other)
    end
  end

  @doc "`DELETE /servers/{server_id}/icon`: the server goes back to having no picture."
  def delete_icon(conn, %{"server_id" => server_id}) do
    with {:ok, _server} <- member_server(conn, server_id),
         {:ok, updated} <- Servers.remove_icon(server_id, conn.assigns.current_account_id) do
      ProtoConn.render(conn, Proto.server(updated))
    end
  end

  def channels(conn, %{"server_id" => server_id}) do
    with {:ok, _server} <- member_server(conn, server_id) do
      channels =
        conn.assigns.current_account_id
        |> Access.viewable_channels(server_id, joined: true)
        |> Enum.map(&Proto.channel(&1.channel))

      ProtoConn.render(conn, %V1.ListChannelsResponse{channels: channels})
    end
  end

  @doc """
  `GET /servers/{server_id}/channels/browse` (architecture 6.8): the
  public channels the caller can see and is not in, with how many are.
  """
  def browse(conn, %{"server_id" => server_id}) do
    with {:ok, _server} <- member_server(conn, server_id) do
      channels =
        server_id
        |> Lumia.Channels.browse(conn.assigns.current_account_id)
        |> Enum.map(fn {channel, count} -> Proto.channel(channel, count) end)

      ProtoConn.render(conn, %V1.ListChannelsResponse{channels: channels})
    end
  end

  @doc "`PATCH /servers/{server_id}/membership`: the caller's own switches there (architecture 6.8)."
  def update_membership(conn, %{"server_id" => server_id}) do
    account_id = conn.assigns.current_account_id

    with {:ok, req} <- ProtoConn.decode(conn, V1.UpdateMembershipRequest),
         changes =
           Map.reject(Map.take(req, [:channel_adds_by_invitation, :server_dms_off]), fn {_, v} ->
             is_nil(v)
           end),
         {:ok, member} <- Servers.update_membership(server_id, account_id, changes) do
      ProtoConn.render(conn, Proto.member(member, role_ids(server_id, account_id)))
    end
  end

  @doc "`POST /servers/{server_id}/dms`: the server DM with a member, started if need be."
  def open_dm(conn, %{"server_id" => server_id}) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.OpenServerDmRequest),
         account_id when is_binary(account_id) <-
           Proto.from_uuid(req.account_id) || {:error, :not_found},
         {:ok, channel} <-
           Lumia.Channels.open_dm(server_id, conn.assigns.current_account_id, account_id) do
      ProtoConn.render(conn, Proto.channel(channel))
    end
  end

  @page_default 50
  @page_max 200

  def members(conn, %{"server_id" => server_id} = params) do
    with {:ok, _server} <- member_server(conn, server_id),
         {:ok, cursor} <- decode_cursor(params["cursor"]) do
      size = params["page_size"] |> page_size()

      members =
        from(m in Member,
          where: m.server_id == ^server_id,
          order_by: [asc: m.joined_at, asc: m.account_id],
          limit: ^(size + 1)
        )
        |> after_cursor(cursor)
        |> Repo.all()

      {members, next_cursor} = page(members, size)

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
        accounts: Enum.map(accounts, &Proto.account/1),
        page: %V1.PageResponse{next_cursor: next_cursor}
      })
    end
  end

  # Keyset pagination over (joined_at, account_id) (protocol section 11).
  # The cursor is opaque to clients: base64 of the last row's key. The
  # query fetches one row past the page to know whether a next page exists.
  defp page_size(nil), do: @page_default

  defp page_size(value) do
    case Integer.parse(value) do
      {n, ""} when n >= 1 -> min(n, @page_max)
      _ -> @page_default
    end
  end

  defp decode_cursor(nil), do: {:ok, nil}
  defp decode_cursor(""), do: {:ok, nil}

  defp decode_cursor(cursor) do
    with {:ok, raw} <- Base.url_decode64(cursor, padding: false),
         [micros, account_id] <- String.split(raw, "|", parts: 2),
         {m, ""} <- Integer.parse(micros),
         {:ok, at} <- DateTime.from_unix(m, :microsecond) do
      {:ok, {at, account_id}}
    else
      _ -> {:error, :invalid_argument}
    end
  end

  defp after_cursor(query, nil), do: query

  defp after_cursor(query, {at, account_id}) do
    from m in query,
      where: m.joined_at > ^at or (m.joined_at == ^at and m.account_id > ^account_id)
  end

  defp page(members, size) when length(members) > size do
    page = Enum.take(members, size)
    last = List.last(page)
    key = "#{DateTime.to_unix(last.joined_at, :microsecond)}|#{last.account_id}"
    {page, Base.url_encode64(key, padding: false)}
  end

  defp page(members, _size), do: {members, ""}

  def update_member(conn, %{"server_id" => server_id, "account_id" => account_id}) do
    actor = conn.assigns.current_account_id

    with {:ok, _server} <- member_server(conn, server_id),
         {:ok, req} <- ProtoConn.decode(conn, V1.UpdateMemberRequest),
         wanted = Enum.map(req.role_ids, &Proto.from_uuid/1),
         true <- Enum.all?(wanted, &is_binary/1) || {:error, :invalid_argument},
         {:ok, member} <- Servers.set_roles(server_id, account_id, wanted, actor) do
      ProtoConn.render(conn, Proto.member(member, role_ids(server_id, account_id)))
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
end
