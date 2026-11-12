defmodule LumiaWeb.ApiTest do
  use LumiaWeb.ConnCase, async: true

  import Lumia.Fixtures

  alias Lumia.Accounts
  alias Lumia.Auth
  alias Lumia.Auth.Totp
  alias Lumia.Proto
  alias Lumia.Servers
  alias Lumia.V1

  @json "application/json"
  @proto "application/x-protobuf"

  defp post_proto(conn, path, struct, encoding \\ :json)

  defp post_proto(conn, path, struct, :json) do
    conn
    |> put_req_header("content-type", @json)
    |> post(path, Lumia.ProtoJSON.encode!(struct))
  end

  defp post_proto(conn, path, struct, :proto) do
    conn
    |> put_req_header("content-type", @proto)
    |> put_req_header("accept", @proto)
    |> post(path, struct.__struct__.encode(struct))
  end

  defp decode(conn, module) do
    case get_resp_header(conn, "content-type") do
      [ct] when ct == @proto <> "; charset=utf-8" or ct == @proto -> module.decode(conn.resp_body)
      _ -> Lumia.ProtoJSON.decode!(conn.resp_body, module)
    end
  end

  defp authed(conn, account) do
    {:ok, _session, tokens} = Auth.create_session(account.id, Ecto.UUID.generate())
    put_req_header(conn, "authorization", "Bearer " <> tokens.access_token)
  end

  describe "public" do
    test "GET /instance is the trust card", %{conn: conn} do
      conn = get(conn, "/api/v1/instance")

      assert %V1.InstanceInfo{recovery_generation: gen, reactions: reactions} =
               decode(conn, V1.InstanceInfo)

      assert gen == Lumia.Instance.recovery_generation()
      assert Enum.map(reactions, & &1.key) == ~w(thumbs_up check heart sparkle)
    end

    test "a magic link is asked for and completed", %{conn: conn} do
      email = "link#{System.unique_integer([:positive])}@example.com"

      {:ok, account} =
        Accounts.create_account(%{
          handle: "link#{System.unique_integer([:positive])}",
          email: email
        })

      assert %V1.InstanceInfo{magic_link_enabled: true} =
               decode(get(conn, "/api/v1/instance"), V1.InstanceInfo)

      asked =
        post_proto(conn, "/api/v1/auth/sign-in/magic-link", %V1.MagicLinkRequest{email: email})

      assert asked.status == 202

      token =
        receive do
          {:email, mail} ->
            mail.text_body
            |> String.split("\n")
            |> Enum.find(&String.match?(&1, ~r/^[A-Za-z0-9_-]{32}$/))
        after
          500 -> flunk("no mail")
        end

      done =
        post_proto(conn, "/api/v1/auth/sign-in/magic-link/complete", %V1.MagicLinkCompleteRequest{
          token: token
        })

      assert %V1.TokenPair{access_token: access} = decode(done, V1.TokenPair)
      assert {:ok, %{account_id: account_id}} = Auth.verify_access(access)
      assert account_id == account.id

      again =
        post_proto(conn, "/api/v1/auth/sign-in/magic-link/complete", %V1.MagicLinkCompleteRequest{
          token: token
        })

      assert again.status == 401
    end

    test "unauthenticated requests get the error envelope", %{conn: conn} do
      conn = get(conn, "/api/v1/account/snapshot")
      assert conn.status == 401
      assert %V1.Error{code: :UNAUTHENTICATED} = decode(conn, V1.Error)
    end
  end

  describe "accounts and sign-in" do
    test "the session made with an account is its device's, so revoking the device ends it", %{
      conn: conn
    } do
      device_id = Ecto.UUID.generate()

      created =
        post_proto(conn, "/api/v1/accounts", %V1.CreateAccountRequest{
          handle: "dev#{System.unique_integer([:positive])}",
          password: "correct horse battery",
          device_id: Proto.uuid(device_id)
        })

      assert %V1.TokenPair{access_token: access} = decode(created, V1.TokenPair)
      assert {:ok, session} = Auth.verify_access(access)
      assert session.device_id == device_id

      Auth.revoke_device(session.account_id, device_id)
      assert {:error, _} = Auth.verify_access(access)

      # A client that names no device still gets a session, one no device owns.
      unnamed =
        post_proto(conn, "/api/v1/accounts", %V1.CreateAccountRequest{
          handle: "nodev#{System.unique_integer([:positive])}",
          password: "correct horse battery"
        })

      assert %V1.TokenPair{access_token: other} = decode(unnamed, V1.TokenPair)
      assert {:ok, %{device_id: minted}} = Auth.verify_access(other)
      assert minted != device_id
    end

    test "create, sign in, refresh, sign out", %{conn: conn} do
      handle = "ryan#{System.unique_integer([:positive])}"

      created =
        post_proto(conn, "/api/v1/accounts", %V1.CreateAccountRequest{
          handle: handle,
          password: "correct horse battery"
        })

      assert created.status == 201

      assert %V1.TokenPair{access_token: access, refresh_token: refresh} =
               decode(created, V1.TokenPair)

      signed =
        post_proto(conn, "/api/v1/auth/sign-in/password", %V1.PasswordSignInRequest{
          handle_or_email: String.upcase(handle),
          password: "correct horse battery"
        })

      assert %V1.TokenPair{} = decode(signed, V1.TokenPair)

      wrong =
        post_proto(conn, "/api/v1/auth/sign-in/password", %V1.PasswordSignInRequest{
          handle_or_email: handle,
          password: "nope"
        })

      assert wrong.status == 401

      refreshed =
        post_proto(conn, "/api/v1/auth/refresh", %V1.RefreshRequest{refresh_token: refresh})

      assert %V1.TokenPair{access_token: access2} = decode(refreshed, V1.TokenPair)
      refute access2 == access

      out =
        conn
        |> put_req_header("authorization", "Bearer " <> access2)
        |> post("/api/v1/auth/sign-out")

      assert out.status == 204

      after_out =
        conn
        |> put_req_header("authorization", "Bearer " <> access2)
        |> get("/api/v1/account/snapshot")

      assert %V1.Error{code: :SESSION_REVOKED} = decode(after_out, V1.Error)
    end

    test "short passwords are rejected with a field detail", %{conn: conn} do
      conn =
        post_proto(conn, "/api/v1/accounts", %V1.CreateAccountRequest{
          handle: "x",
          password: "short"
        })

      assert conn.status == 400
      assert %V1.Error{code: :INVALID_ARGUMENT} = decode(conn, V1.Error)
    end

    test "sessions can be listed and revoked", %{conn: conn} do
      account = account()
      {:ok, s1, t1} = Auth.create_session(account.id, Ecto.UUID.generate())
      {:ok, s2, _} = Auth.create_session(account.id, Ecto.UUID.generate())
      conn = put_req_header(conn, "authorization", "Bearer " <> t1.access_token)

      listed = get(conn, "/api/v1/sessions")

      assert %V1.ListSessionsResponse{sessions: sessions} =
               decode(listed, V1.ListSessionsResponse)

      assert Enum.map(sessions, &Proto.from_uuid(&1.session_id)) |> Enum.sort() ==
               Enum.sort([s1.id, s2.id])

      assert Enum.find(sessions, & &1.current).session_id == Proto.uuid(s1.id)

      assert delete(conn, "/api/v1/sessions/#{s2.id}").status == 204
      assert delete(conn, "/api/v1/sessions/#{Ecto.UUID.generate()}").status == 404
    end
  end

  describe "TOTP as a second factor" do
    test "enrol, confirm, then sign-in and reauth require a code", %{conn: conn} do
      handle = "totp#{System.unique_integer([:positive])}"
      password = "correct horse battery"

      created =
        post_proto(conn, "/api/v1/accounts", %V1.CreateAccountRequest{
          handle: handle,
          password: password
        })

      %V1.TokenPair{access_token: access} = decode(created, V1.TokenPair)
      authed = put_req_header(conn, "authorization", "Bearer " <> access)

      enrolled = post(authed, "/api/v1/auth/totp/enroll")

      assert %V1.TotpEnrollResponse{secret: secret, otpauth_uri: uri} =
               decode(enrolled, V1.TotpEnrollResponse)

      assert String.starts_with?(uri, "otpauth://totp/")
      raw = Base.decode32!(secret, padding: false)

      wrong =
        post_proto(authed, "/api/v1/auth/totp/confirm", %V1.TotpConfirmRequest{code: "000000"})

      assert wrong.status == 401

      ok =
        post_proto(authed, "/api/v1/auth/totp/confirm", %V1.TotpConfirmRequest{
          code: Totp.code(raw)
        })

      assert ok.status == 204

      # Sign-in now asks for the second factor, then accepts the code.
      asked =
        post_proto(conn, "/api/v1/auth/sign-in/password", %V1.PasswordSignInRequest{
          handle_or_email: handle,
          password: password
        })

      assert asked.status == 400

      assert %V1.Error{code: :FAILED_PRECONDITION, details: %{"mfa" => "totp"}} =
               decode(asked, V1.Error)

      refused =
        post_proto(conn, "/api/v1/auth/sign-in/password", %V1.PasswordSignInRequest{
          handle_or_email: handle,
          password: password,
          totp_code: "123456"
        })

      assert refused.status == 401

      signed =
        post_proto(conn, "/api/v1/auth/sign-in/password", %V1.PasswordSignInRequest{
          handle_or_email: handle,
          password: password,
          totp_code: Totp.code(raw)
        })

      assert %V1.TokenPair{} = decode(signed, V1.TokenPair)

      # Reauth by TOTP (identity spec 10).
      reauthed =
        post_proto(authed, "/api/v1/auth/reauth", %V1.ReauthRequest{
          credential: {:totp_code, Totp.code(raw)}
        })

      assert %V1.ReauthResponse{reauth_token: token} = decode(reauthed, V1.ReauthResponse)
      assert token != ""

      bad_reauth =
        post_proto(authed, "/api/v1/auth/reauth", %V1.ReauthRequest{
          credential: {:totp_code, "000000"}
        })

      assert bad_reauth.status == 401
    end
  end

  describe "servers, channels, members, roles" do
    setup %{conn: conn} do
      owner = account("owner")
      {:ok, server} = Servers.create_server(owner.id, "Acme")
      {:ok, general} = Servers.create_channel(server.id, owner.id, "general")
      %{conn: authed(conn, owner), owner: owner, server: server, general: general}
    end

    test "account snapshot lists servers, memberships and viewable channels", ctx do
      conn = get(ctx.conn, "/api/v1/account/snapshot")

      assert %V1.AccountSnapshot{servers: [server], memberships: [member], channels: [channel]} =
               decode(conn, V1.AccountSnapshot)

      assert server.name == "Acme"
      assert member.kind == :MEMBER
      assert channel.name == "general"
    end

    test "channels at one position are listed in the order they were made", ctx do
      {:ok, _} = Servers.create_channel(ctx.server.id, ctx.owner.id, "design")
      {:ok, _} = Servers.create_channel(ctx.server.id, ctx.owner.id, "archive")

      names = fn ->
        Enum.map(
          decode(get(ctx.conn, "/api/v1/account/snapshot"), V1.AccountSnapshot).channels,
          & &1.name
        )
      end

      assert names.() == ["general", "design", "archive"]

      # An update moves the row in the heap, which is the order an unordered tie comes back in.
      import Ecto.Query

      Lumia.Repo.update_all(from(c in Lumia.Events.Channel, where: c.id == ^ctx.general.id),
        set: [topic: "moved"]
      )

      assert names.() == ["general", "design", "archive"]
    end

    test "create a server and a channel over the API, in protobuf", ctx do
      created =
        post_proto(ctx.conn, "/api/v1/servers", %V1.CreateServerRequest{name: "Beta"}, :proto)

      assert created.status == 201
      assert %V1.Server{server_id: server_id, name: "Beta"} = decode(created, V1.Server)

      channel =
        post_proto(
          ctx.conn,
          "/api/v1/channels",
          %V1.CreateChannelRequest{server_id: server_id, name: "ops", topic: "on call"},
          :proto
        )

      assert %V1.Channel{name: "ops", topic: "on call", acl_version: 1} =
               decode(channel, V1.Channel)
    end

    test "creating a channel needs manage_channels in that server", ctx do
      member = account("member")
      {:ok, _} = Servers.add_member(ctx.server.id, member.id, ctx.owner.id)
      stranger = account("stranger")
      req = %V1.CreateChannelRequest{server_id: Proto.uuid(ctx.server.id), name: "ops"}

      assert post_proto(authed(ctx.conn, member), "/api/v1/channels", req).status == 403
      assert post_proto(authed(ctx.conn, stranger), "/api/v1/channels", req).status == 403
      assert post_proto(ctx.conn, "/api/v1/channels", req).status == 201
    end

    test "a malformed id in the path is NOT_FOUND, not a crash", ctx do
      assert get(ctx.conn, "/api/v1/servers/not-a-uuid/channels").status == 404
      assert get(ctx.conn, "/api/v1/channels/channels/snapshot").status == 404
      assert %V1.Error{code: :NOT_FOUND} = decode(get(ctx.conn, "/api/v1/servers/x"), V1.Error)
    end

    test "a non-member gets NOT_FOUND for a server and its channels", ctx do
      outsider = account("outsider") |> then(&authed(build_conn(), &1))
      assert get(outsider, "/api/v1/servers/#{ctx.server.id}").status == 404
      assert get(outsider, "/api/v1/channels/#{ctx.general.id}/snapshot").status == 404
      assert get(outsider, "/api/v1/channels/#{ctx.general.id}/events").status == 404
    end

    @tag failure_injection: [authorization: 8]
    test "an instance admin without membership gets NOT_FOUND, private channel included", ctx do
      everyone =
        Lumia.Repo.get_by!(Lumia.Servers.Role, server_id: ctx.server.id, is_everyone: true)

      :ok =
        Servers.set_overwrite(ctx.general.id, ctx.owner.id, {:role, everyone.id}, [], [
          :view_channel
        ])

      admin =
        account("instance-admin")
        |> Ecto.Changeset.change(instance_role: "admin")
        |> Lumia.Repo.update!()

      conn = authed(build_conn(), admin)
      assert get(conn, "/api/v1/servers/#{ctx.server.id}").status == 404
      assert get(conn, "/api/v1/channels/#{ctx.general.id}/snapshot").status == 404
      assert get(conn, "/api/v1/channels/#{ctx.general.id}/events").status == 404
      assert get(conn, "/api/v1/channels/#{ctx.general.id}/messages").status == 404
    end

    test "roles: create, list, update, delete with hierarchy enforced", ctx do
      role = %V1.Role{name: "mods", position: 3, permissions: [:MANAGE_MESSAGES]}

      created =
        post_proto(ctx.conn, "/api/v1/servers/#{ctx.server.id}/roles", %V1.UpsertRoleRequest{
          role: role
        })

      assert created.status == 201

      assert %V1.Role{role_id: role_id, permissions: [:MANAGE_MESSAGES]} =
               decode(created, V1.Role)

      rid = Proto.from_uuid(role_id)

      listed = get(ctx.conn, "/api/v1/servers/#{ctx.server.id}/roles")
      assert %V1.ListRolesResponse{roles: roles} = decode(listed, V1.ListRolesResponse)
      assert Enum.any?(roles, & &1.is_everyone)
      assert Enum.any?(roles, &(&1.name == "mods"))

      updated =
        ctx.conn
        |> put_req_header("content-type", @json)
        |> put(
          "/api/v1/servers/#{ctx.server.id}/roles/#{rid}",
          Lumia.ProtoJSON.encode!(%V1.UpsertRoleRequest{role: %{role | name: "moderators"}})
        )

      assert %V1.Role{name: "moderators"} = decode(updated, V1.Role)

      # A member below the role cannot delete it.
      member = account("member")
      {:ok, _} = Servers.add_member(ctx.server.id, member.id, ctx.owner.id)

      denied =
        build_conn() |> authed(member) |> delete("/api/v1/servers/#{ctx.server.id}/roles/#{rid}")

      assert denied.status == 403

      assert delete(ctx.conn, "/api/v1/servers/#{ctx.server.id}/roles/#{rid}").status == 204
    end

    test "members paginate by keyset with an opaque cursor (protocol section 11)", ctx do
      for i <- 1..5 do
        {:ok, _} = Servers.add_member(ctx.server.id, account("m#{i}").id, ctx.owner.id)
      end

      first = get(ctx.conn, "/api/v1/servers/#{ctx.server.id}/members?page_size=4")

      assert %V1.ListMembersResponse{members: page1, page: %V1.PageResponse{next_cursor: cursor}} =
               decode(first, V1.ListMembersResponse)

      assert length(page1) == 4 and cursor != ""

      second =
        get(ctx.conn, "/api/v1/servers/#{ctx.server.id}/members?page_size=4&cursor=#{cursor}")

      assert %V1.ListMembersResponse{members: page2, page: %V1.PageResponse{next_cursor: ""}} =
               decode(second, V1.ListMembersResponse)

      assert length(page2) == 2
      ids = Enum.map(page1 ++ page2, & &1.account_id)
      assert length(Enum.uniq(ids)) == 6

      bad = get(ctx.conn, "/api/v1/servers/#{ctx.server.id}/members?cursor=not-a-cursor")
      assert bad.status == 400
    end

    test "members: list, assign roles, remove", ctx do
      alice = account("alice")
      {:ok, _} = Servers.add_member(ctx.server.id, alice.id, ctx.owner.id)

      {:ok, role} =
        Servers.create_role(ctx.server.id, ctx.owner.id, %{name: "helpers", permissions: []})

      listed = get(ctx.conn, "/api/v1/servers/#{ctx.server.id}/members")

      assert %V1.ListMembersResponse{members: members, accounts: accounts} =
               decode(listed, V1.ListMembersResponse)

      assert length(members) == 2 and length(accounts) == 2

      patched =
        ctx.conn
        |> put_req_header("content-type", @json)
        |> patch(
          "/api/v1/servers/#{ctx.server.id}/members/#{alice.id}",
          Lumia.ProtoJSON.encode!(%V1.UpdateMemberRequest{role_ids: [Proto.uuid(role.id)]})
        )

      assert %V1.Member{role_ids: [rid]} = decode(patched, V1.Member)
      assert Proto.from_uuid(rid) == role.id
      assert "membership_updated" in Accounts.event_kinds(alice.id)

      assert delete(ctx.conn, "/api/v1/servers/#{ctx.server.id}/members/#{alice.id}").status ==
               204

      refute Servers.member_for(ctx.server.id, alice.id)
    end

    test "overwrites make a channel private and the API reflects it", ctx do
      alice = account("alice")
      {:ok, _} = Servers.add_member(ctx.server.id, alice.id, ctx.owner.id)

      everyone =
        Lumia.Repo.get_by!(Lumia.Servers.Role, server_id: ctx.server.id, is_everyone: true)

      resp =
        ctx.conn
        |> put_req_header("content-type", @json)
        |> put(
          "/api/v1/channels/#{ctx.general.id}/overwrites",
          Lumia.ProtoJSON.encode!(%V1.Overwrite{
            target: {:role_id, Proto.uuid(everyone.id)},
            deny: [:VIEW_CHANNEL]
          })
        )

      assert resp.status == 204
      alice_conn = authed(build_conn(), alice)
      assert get(alice_conn, "/api/v1/channels/#{ctx.general.id}/snapshot").status == 404

      assert %V1.ListChannelsResponse{channels: []} =
               decode(
                 get(alice_conn, "/api/v1/servers/#{ctx.server.id}/channels"),
                 V1.ListChannelsResponse
               )

      assert %V1.ListChannelsResponse{channels: [_]} =
               decode(
                 get(ctx.conn, "/api/v1/servers/#{ctx.server.id}/channels"),
                 V1.ListChannelsResponse
               )
    end
  end

  describe "commands, events, snapshots, messages" do
    setup %{conn: conn} do
      owner = account("owner")
      {:ok, server} = Servers.create_server(owner.id, "Acme")
      {:ok, general} = Servers.create_channel(server.id, owner.id, "general")
      %{conn: authed(conn, owner), owner: owner, server: server, general: general}
    end

    test "a command is acked and visible in events, snapshot and messages", ctx do
      cmd = send_message(ctx.general.id, "hello over http")
      acked = post_proto(ctx.conn, "/api/v1/channels/#{ctx.general.id}/commands", cmd, :proto)
      assert %V1.Ack{seq: 1, command_id: command_id} = decode(acked, V1.Ack)
      assert command_id == cmd.command_id

      # Replay is idempotent over HTTP too.
      again = post_proto(ctx.conn, "/api/v1/channels/#{ctx.general.id}/commands", cmd, :proto)
      assert %V1.Ack{seq: 1} = decode(again, V1.Ack)

      events = get(ctx.conn, "/api/v1/channels/#{ctx.general.id}/events?after_seq=0")

      assert %V1.ListEventsResponse{
               events: [%V1.Event{seq: 1}],
               last_seq: 1,
               truncated_before: false
             } = decode(events, V1.ListEventsResponse)

      snap = get(ctx.conn, "/api/v1/channels/#{ctx.general.id}/snapshot")

      assert %V1.ChannelSnapshot{seq: 1, messages: [%V1.Message{body: "hello over http"}]} =
               decode(snap, V1.ChannelSnapshot)

      messages = get(ctx.conn, "/api/v1/channels/#{ctx.general.id}/messages?limit=10")

      assert %V1.ListMessagesResponse{messages: [_], has_more: false} =
               decode(messages, V1.ListMessagesResponse)
    end

    test "scrollback pages by seq with has_more", ctx do
      actor = %{account_id: ctx.owner.id, device_id: nil}

      for i <- 1..5,
          do:
            {:ok, _} =
              Lumia.Events.append(ctx.general.id, actor, send_message(ctx.general.id, "m#{i}"))

      page =
        decode(
          get(ctx.conn, "/api/v1/channels/#{ctx.general.id}/messages?limit=2"),
          V1.ListMessagesResponse
        )

      assert Enum.map(page.messages, & &1.body) == ["m4", "m5"]
      assert page.has_more

      older =
        decode(
          get(ctx.conn, "/api/v1/channels/#{ctx.general.id}/messages?limit=2&before_seq=4"),
          V1.ListMessagesResponse
        )

      assert Enum.map(older.messages, & &1.body) == ["m2", "m3"]
    end

    test "a command without SEND_MESSAGES is denied; a malformed body is invalid", ctx do
      member = account("member")
      {:ok, _} = Servers.add_member(ctx.server.id, member.id, ctx.owner.id)

      everyone =
        Lumia.Repo.get_by!(Lumia.Servers.Role, server_id: ctx.server.id, is_everyone: true)

      :ok =
        Servers.set_overwrite(ctx.general.id, ctx.owner.id, {:role, everyone.id}, [], [
          :send_messages
        ])

      denied =
        build_conn()
        |> authed(member)
        |> post_proto("/api/v1/channels/#{ctx.general.id}/commands", send_message(ctx.general.id))

      assert denied.status == 403

      bad =
        ctx.conn
        |> put_req_header("content-type", @proto)
        |> post("/api/v1/channels/#{ctx.general.id}/commands", "not protobuf")

      assert bad.status == 400
      assert %V1.Error{code: :INVALID_ARGUMENT} = decode(bad, V1.Error)
    end

    test "account events are listed after a cursor", ctx do
      {:ok, _} = Accounts.append_event(ctx.owner.id, {:profile_updated, %V1.ProfileUpdated{}})
      resp = get(ctx.conn, "/api/v1/account/events?after_seq=0")

      assert %V1.ListAccountEventsResponse{events: events, last_seq: last} =
               decode(resp, V1.ListAccountEventsResponse)

      assert last >= 1
      assert Enum.any?(events, &match?({:profile_updated, _}, &1.payload))
    end
  end
end
