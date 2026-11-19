defmodule LumiaWeb.RolesApiTest do
  @moduledoc """
  Roles, titles and the member list over HTTP (protocol 9, architecture
  7.5): what a request may carry and what it is answered.
  """

  use LumiaWeb.ConnCase, async: true

  import Ecto.Query
  import Lumia.Fixtures

  alias Lumia.Auth
  alias Lumia.Proto
  alias Lumia.Servers
  alias Lumia.V1

  @proto "application/x-protobuf"

  defp authed(conn, account) do
    {:ok, _session, tokens} = Auth.create_session(account.id, Ecto.UUID.generate())

    conn
    |> put_req_header("authorization", "Bearer " <> tokens.access_token)
    |> put_req_header("accept", @proto)
    |> put_req_header("content-type", @proto)
  end

  defp send_proto(conn, account, method, path, %module{} = message) do
    conn
    |> authed(account)
    |> dispatch(@endpoint, method, "/api/v1" <> path, module.encode(message))
  end

  defp error(conn), do: V1.Error.decode(conn.resp_body)

  setup do
    owner = account("owner")
    member = account("member")
    {:ok, server} = Servers.create_server(owner.id, "Acme")
    {:ok, _} = Servers.add_member(server.id, member.id, owner.id)
    %{owner: owner, member: member, server: server}
  end

  defp role(ctx, name, permissions \\ []) do
    {:ok, role} =
      Servers.create_role(ctx.server.id, ctx.owner.id, %{name: name, permissions: permissions})

    role
  end

  describe "POST and PUT /servers/{id}/roles" do
    test "a role is made last, with its colour and tag switch, whatever position is sent", ctx do
      role(ctx, "first")

      conn =
        send_proto(
          build_conn(),
          ctx.owner,
          :post,
          "/servers/#{ctx.server.id}/roles",
          %V1.UpsertRoleRequest{
            role: %V1.Role{name: "Design", color: "mint", hide_tag: true, position: 0}
          }
        )

      assert conn.status == 201

      assert %V1.Role{name: "Design", color: "mint", hide_tag: true, position: 2} =
               V1.Role.decode(conn.resp_body)
    end

    test "a name or a colour that will not do is named in the refusal", ctx do
      role(ctx, "Design")
      path = "/servers/#{ctx.server.id}/roles"

      for {role, field} <- [
            {%V1.Role{name: ""}, "name"},
            {%V1.Role{name: "ok", color: "#fff"}, "color"}
          ] do
        conn = send_proto(build_conn(), ctx.owner, :post, path, %V1.UpsertRoleRequest{role: role})
        assert conn.status == 400
        assert %V1.Error{code: :INVALID_ARGUMENT, details: %{"field" => ^field}} = error(conn)
      end

      conn =
        send_proto(build_conn(), ctx.owner, :post, path, %V1.UpsertRoleRequest{
          role: %V1.Role{name: "design"}
        })

      assert conn.status == 409
      assert %V1.Error{code: :ALREADY_EXISTS} = error(conn)
    end

    test "a PUT is the role whole, and its place stays the instance's", ctx do
      first = role(ctx, "first")
      role(ctx, "second")

      conn =
        send_proto(
          build_conn(),
          ctx.owner,
          :put,
          "/servers/#{ctx.server.id}/roles/#{first.id}",
          %V1.UpsertRoleRequest{
            role: %V1.Role{name: "one", color: "sky", position: 7, permissions: [:CREATE_INVITES]}
          }
        )

      assert %V1.Role{name: "one", color: "sky", position: 1, permissions: [:CREATE_INVITES]} =
               V1.Role.decode(conn.resp_body)
    end
  end

  describe "PATCH /servers/{id}/roles/{role_id}" do
    test "changes what it names and nothing else of the role", ctx do
      design = role(ctx, "Design", [:manage_messages, :create_invites])
      path = "/servers/#{ctx.server.id}/roles/#{design.id}"

      conn =
        send_proto(build_conn(), ctx.owner, :patch, path, %V1.UpdateRoleRequest{
          color: "sky",
          grant: [:MANAGE_CHANNELS],
          revoke: [:CREATE_INVITES]
        })

      assert %V1.Role{name: "Design", color: "sky", hide_tag: false, permissions: permissions} =
               V1.Role.decode(conn.resp_body)

      assert Enum.sort(permissions) == [:MANAGE_CHANNELS, :MANAGE_MESSAGES]

      # A colour named and empty is none; a name alone leaves it.
      conn = send_proto(build_conn(), ctx.owner, :patch, path, %V1.UpdateRoleRequest{color: ""})
      assert %V1.Role{color: ""} = V1.Role.decode(conn.resp_body)

      conn =
        send_proto(build_conn(), ctx.owner, :patch, path, %V1.UpdateRoleRequest{
          name: "Designers",
          hide_tag: true
        })

      assert %V1.Role{name: "Designers", color: "", hide_tag: true, permissions: ^permissions} =
               V1.Role.decode(conn.resp_body)
    end

    test "refuses a permission switched both ways, and a member without MANAGE_ROLES", ctx do
      design = role(ctx, "Design")
      path = "/servers/#{ctx.server.id}/roles/#{design.id}"

      conn =
        send_proto(build_conn(), ctx.owner, :patch, path, %V1.UpdateRoleRequest{
          grant: [:CREATE_INVITES],
          revoke: [:CREATE_INVITES]
        })

      assert conn.status == 400
      assert %V1.Error{details: %{"field" => "permissions"}} = error(conn)

      conn =
        send_proto(build_conn(), ctx.member, :patch, path, %V1.UpdateRoleRequest{name: "Mine"})

      assert conn.status == 403
    end
  end

  describe "PUT /servers/{id}/role-order" do
    test "answers the roles in their new order, @everyone last", ctx do
      [a, b] = for name <- ~w(a b), do: role(ctx, name)

      conn =
        send_proto(
          build_conn(),
          ctx.owner,
          :put,
          "/servers/#{ctx.server.id}/role-order",
          %V1.ReorderRolesRequest{
            role_ids: [Proto.uuid(b.id), Proto.uuid(a.id)]
          }
        )

      assert conn.status == 200
      assert %V1.ListRolesResponse{roles: roles} = V1.ListRolesResponse.decode(conn.resp_body)

      assert Enum.map(roles, &{&1.name, &1.position}) == [
               {"b", 1},
               {"a", 2},
               {"@everyone", 1_000_000}
             ]

      listed = build_conn() |> authed(ctx.member) |> get("/api/v1/servers/#{ctx.server.id}/roles")
      assert %V1.ListRolesResponse{roles: ^roles} = V1.ListRolesResponse.decode(listed.resp_body)
    end

    test "refuses an order that is not the roles, and a member without MANAGE_ROLES", ctx do
      [a, b] = for name <- ~w(a b), do: role(ctx, name)
      path = "/servers/#{ctx.server.id}/role-order"

      for ids <- [[Proto.uuid(a.id)], [Proto.uuid(a.id), "short"]] do
        conn =
          send_proto(build_conn(), ctx.owner, :put, path, %V1.ReorderRolesRequest{role_ids: ids})

        assert conn.status == 400
        assert %V1.Error{details: %{"field" => "role_ids"}} = error(conn)
      end

      conn =
        send_proto(build_conn(), ctx.member, :put, path, %V1.ReorderRolesRequest{
          role_ids: [Proto.uuid(b.id), Proto.uuid(a.id)]
        })

      assert conn.status == 403
    end
  end

  describe "PATCH /servers/{id}/members/{account_id}" do
    test "a title alone leaves the roles as they are, and roles alone the title", ctx do
      mods = role(ctx, "mods")
      path = "/servers/#{ctx.server.id}/members/#{ctx.member.id}"

      conn =
        send_proto(build_conn(), ctx.owner, :patch, path, %V1.UpdateMemberRequest{
          roles: %V1.RoleIds{role_ids: [Proto.uuid(mods.id)]}
        })

      assert %V1.Member{role_ids: [_], title: ""} = V1.Member.decode(conn.resp_body)

      conn =
        send_proto(build_conn(), ctx.owner, :patch, path, %V1.UpdateMemberRequest{
          title: "Designer"
        })

      assert %V1.Member{role_ids: [_], title: "Designer"} = V1.Member.decode(conn.resp_body)

      conn =
        send_proto(build_conn(), ctx.owner, :patch, path, %V1.UpdateMemberRequest{
          roles: %V1.RoleIds{role_ids: []}
        })

      assert %V1.Member{role_ids: [], title: "Designer"} = V1.Member.decode(conn.resp_body)

      conn = send_proto(build_conn(), ctx.owner, :patch, path, %V1.UpdateMemberRequest{title: ""})
      assert %V1.Member{title: ""} = V1.Member.decode(conn.resp_body)
    end

    test "roles to give and to take leave the member's others as they are", ctx do
      mods = role(ctx, "mods")
      events = role(ctx, "events")
      design = role(ctx, "design")

      {:ok, _} =
        Servers.set_roles(ctx.server.id, ctx.member.id, [mods.id, events.id], ctx.owner.id)

      path = "/servers/#{ctx.server.id}/members/#{ctx.member.id}"

      conn =
        send_proto(build_conn(), ctx.owner, :patch, path, %V1.UpdateMemberRequest{
          add_role_ids: [Proto.uuid(design.id)],
          remove_role_ids: [Proto.uuid(mods.id)],
          title: "Designer"
        })

      assert %V1.Member{role_ids: role_ids, title: "Designer"} = V1.Member.decode(conn.resp_body)

      assert role_ids |> Enum.map(&Proto.from_uuid/1) |> Enum.sort() ==
               Enum.sort([events.id, design.id])
    end

    test "the whole set and roles to give or take are not sent together", ctx do
      mods = role(ctx, "mods")
      path = "/servers/#{ctx.server.id}/members/#{ctx.member.id}"

      conn =
        send_proto(build_conn(), ctx.owner, :patch, path, %V1.UpdateMemberRequest{
          roles: %V1.RoleIds{role_ids: []},
          add_role_ids: [Proto.uuid(mods.id)]
        })

      assert conn.status == 400
      assert %V1.Error{details: %{"field" => "roles"}} = error(conn)
    end

    test "a title refused undoes the roles sent with it", ctx do
      mods = role(ctx, "mods")

      conn =
        send_proto(
          build_conn(),
          ctx.owner,
          :patch,
          "/servers/#{ctx.server.id}/members/#{ctx.member.id}",
          %V1.UpdateMemberRequest{
            roles: %V1.RoleIds{role_ids: [Proto.uuid(mods.id)]},
            title: String.duplicate("a", 33)
          }
        )

      assert conn.status == 400
      assert %V1.Error{details: %{"field" => "title"}} = error(conn)
      assert Servers.member_for(ctx.server.id, ctx.member.id).role_ids == []
    end

    test "a member sets nobody else's title", ctx do
      conn =
        send_proto(
          build_conn(),
          ctx.member,
          :patch,
          "/servers/#{ctx.server.id}/members/#{ctx.owner.id}",
          %V1.UpdateMemberRequest{title: "Nope"}
        )

      assert conn.status == 403
    end
  end

  describe "PATCH /servers/{id}/membership" do
    test "the caller's own title", ctx do
      path = "/servers/#{ctx.server.id}/membership"

      conn =
        send_proto(build_conn(), ctx.member, :patch, path, %V1.UpdateMembershipRequest{
          title: "Designer"
        })

      assert %V1.Member{title: "Designer"} = V1.Member.decode(conn.resp_body)

      conn =
        send_proto(build_conn(), ctx.member, :patch, path, %V1.UpdateMembershipRequest{
          title: "two\nlines"
        })

      assert conn.status == 400
      assert %V1.Error{details: %{"field" => "title"}} = error(conn)
    end
  end

  describe "GET /servers/{id}/members" do
    test "carries each member's roles and title", ctx do
      mods = role(ctx, "mods")
      {:ok, _} = Servers.set_roles(ctx.server.id, ctx.member.id, [mods.id], ctx.owner.id)
      {:ok, _} = Servers.update_membership(ctx.server.id, ctx.member.id, %{title: "Designer"})

      conn = build_conn() |> authed(ctx.member) |> get("/api/v1/servers/#{ctx.server.id}/members")

      assert %V1.ListMembersResponse{members: members} =
               V1.ListMembersResponse.decode(conn.resp_body)

      assert %V1.Member{role_ids: [role_id], title: "Designer"} =
               Enum.find(members, &(Proto.from_uuid(&1.account_id) == ctx.member.id))

      assert Proto.from_uuid(role_id) == mods.id
    end

    test "query keeps those whose handle or display name begins with it", ctx do
      {:ok, ada} =
        Lumia.Accounts.create_account(%{handle: "ada#{System.unique_integer([:positive])}"})

      {:ok, _} = Servers.add_member(ctx.server.id, ada.id, ctx.owner.id)

      Lumia.Repo.update!(Ecto.Changeset.change(ctx.member, display_name: "Zed 100%"))

      found = fn query ->
        conn =
          build_conn()
          |> authed(ctx.owner)
          |> get("/api/v1/servers/#{ctx.server.id}/members", %{"query" => query})

        %V1.ListMembersResponse{members: members, accounts: accounts} =
          V1.ListMembersResponse.decode(conn.resp_body)

        assert length(members) == length(accounts)
        Enum.map(members, &Proto.from_uuid(&1.account_id))
      end

      assert found.("ADA") == [ada.id]
      assert found.("zed") == [ctx.member.id]
      assert found.("zed 100%") == [ctx.member.id]
      # The query's own wildcards match themselves and nothing else.
      assert found.("%") == []
      assert found.("_ed") == []
      assert length(found.("")) == 3
    end
  end

  describe "PATCH /servers/{id}" do
    test "member_tag says whether a tag is a role or a title, and is audited", ctx do
      path = "/servers/#{ctx.server.id}"

      conn =
        send_proto(build_conn(), ctx.owner, :patch, path, %V1.UpdateServerRequest{
          member_tag: :MEMBER_TAG_TITLE
        })

      assert %V1.Server{member_tag: :MEMBER_TAG_TITLE} = V1.Server.decode(conn.resp_body)

      # Left out, it is left as it is.
      conn =
        send_proto(build_conn(), ctx.owner, :patch, path, %V1.UpdateServerRequest{name: "Acme 2"})

      assert %V1.Server{member_tag: :MEMBER_TAG_TITLE, name: "Acme 2"} =
               V1.Server.decode(conn.resp_body)

      conn =
        send_proto(build_conn(), ctx.owner, :patch, path, %V1.UpdateServerRequest{
          member_tag: :MEMBER_TAG_ROLE
        })

      assert %V1.Server{member_tag: :MEMBER_TAG_ROLE} = V1.Server.decode(conn.resp_body)

      assert Lumia.Repo.aggregate(
               from(e in Lumia.Audit.Entry,
                 where: e.scope_id == ^ctx.server.id and e.action == "server.member_tag"
               ),
               :count
             ) == 2

      conn =
        send_proto(build_conn(), ctx.member, :patch, path, %V1.UpdateServerRequest{
          member_tag: :MEMBER_TAG_TITLE
        })

      assert conn.status == 403
    end
  end
end
