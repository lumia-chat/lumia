defmodule LumiaWeb.InviteApiTest do
  use LumiaWeb.ConnCase, async: true

  import Lumia.Fixtures

  alias Lumia.Auth
  alias Lumia.Invites
  alias Lumia.Proto
  alias Lumia.Servers
  alias Lumia.V1

  @json "application/json"

  defp post_proto(conn, path, struct) do
    conn
    |> put_req_header("content-type", @json)
    |> post(path, Protobuf.JSON.encode!(struct))
  end

  defp decode(conn, module), do: Protobuf.JSON.decode!(conn.resp_body, module)

  defp authed(conn, account) do
    {:ok, _session, tokens} = Auth.create_session(account.id, Ecto.UUID.generate())
    put_req_header(conn, "authorization", "Bearer " <> tokens.access_token)
  end

  setup %{conn: conn} do
    owner = account("owner")
    {:ok, server} = Servers.create_server(owner.id, "Acme")
    {:ok, general} = Servers.create_channel(server.id, owner.id, "general")
    %{conn: conn, owner: owner, server: server, general: general}
  end

  describe "creating" do
    test "the owner mints an invite for the server", ctx do
      conn =
        post_proto(authed(ctx.conn, ctx.owner), "/api/v1/invites", %V1.CreateInviteRequest{
          server_id: Proto.uuid(ctx.server.id),
          max_uses: 2
        })

      assert conn.status == 201
      assert %V1.Invite{code: code, max_uses: 2, uses: 0} = decode(conn, V1.Invite)
      assert String.length(code) == 10
    end

    test "a member without create_invites is refused", ctx do
      member = account("member")
      {:ok, _} = Servers.add_member(ctx.server.id, member.id, ctx.owner.id)

      conn =
        post_proto(authed(ctx.conn, member), "/api/v1/invites", %V1.CreateInviteRequest{
          server_id: Proto.uuid(ctx.server.id)
        })

      assert conn.status == 403
    end

    test "a channel of another server is not found", ctx do
      {:ok, other} = Servers.create_server(ctx.owner.id, "Other")
      {:ok, elsewhere} = Servers.create_channel(other.id, ctx.owner.id, "elsewhere")

      conn =
        post_proto(authed(ctx.conn, ctx.owner), "/api/v1/invites", %V1.CreateInviteRequest{
          server_id: Proto.uuid(ctx.server.id),
          channel_id: Proto.uuid(elsewhere.id)
        })

      assert conn.status == 404
    end
  end

  describe "the preview" do
    test "is public and names the server, the count and the instance", ctx do
      {:ok, invite} = Invites.create(ctx.server.id, ctx.owner.id, %{channel_id: ctx.general.id})
      conn = get(ctx.conn, "/api/v1/invites/#{invite.code}")

      assert %V1.InvitePreview{server: server, channel: channel, member_count: 1, instance: info} =
               decode(conn, V1.InvitePreview)

      assert server.name == "Acme"
      assert channel.name == "general"
      assert info.host != ""
    end

    test "an unknown, expired or spent code is not found", ctx do
      assert get(ctx.conn, "/api/v1/invites/nope").status == 404

      {:ok, expired} =
        Invites.create(ctx.server.id, ctx.owner.id, %{
          expires_at: DateTime.add(DateTime.utc_now(), -1, :second)
        })

      assert get(ctx.conn, "/api/v1/invites/#{expired.code}").status == 404

      {:ok, once} = Invites.create(ctx.server.id, ctx.owner.id, %{max_uses: 1})
      {:ok, _} = Invites.accept(once.code, account("first").id)
      assert get(ctx.conn, "/api/v1/invites/#{once.code}").status == 404
    end
  end

  describe "accepting" do
    test "joins the caller as a member and counts the use", ctx do
      {:ok, invite} = Invites.create(ctx.server.id, ctx.owner.id, %{max_uses: 1})
      joiner = account("joiner")
      conn = post(authed(ctx.conn, joiner), "/api/v1/invites/#{invite.code}/accept")

      assert %V1.Member{kind: :MEMBER} = decode(conn, V1.Member)
      assert Servers.member_for(ctx.server.id, joiner.id).kind == :member
      assert Lumia.Repo.get!(Invites.Invite, invite.id).uses == 1

      # The last use is gone for the next caller.
      conn = post(authed(ctx.conn, account("late")), "/api/v1/invites/#{invite.code}/accept")
      assert conn.status == 404
    end

    test "a member accepting again keeps the membership and the count", ctx do
      {:ok, invite} = Invites.create(ctx.server.id, ctx.owner.id, %{max_uses: 1})
      conn = post(authed(ctx.conn, ctx.owner), "/api/v1/invites/#{invite.code}/accept")

      assert %V1.Member{} = decode(conn, V1.Member)
      assert Lumia.Repo.get!(Invites.Invite, invite.id).uses == 0
    end

    test "a guest invite makes a guest with the expiry", ctx do
      {:ok, invite} =
        Invites.create(ctx.server.id, ctx.owner.id, %{as_guest: true, guest_days: 7})

      guest = account("guest")
      conn = post(authed(ctx.conn, guest), "/api/v1/invites/#{invite.code}/accept")

      assert %V1.Member{kind: :GUEST, expires_at: %{}} = decode(conn, V1.Member)
      member = Lumia.Repo.get_by!(Servers.Member, server_id: ctx.server.id, account_id: guest.id)
      assert DateTime.diff(member.expires_at, DateTime.utc_now(), :day) in 6..7
    end
  end

  describe "account creation with a code" do
    test "joins the server, and is admitted when registration is closed", ctx do
      {:ok, invite} = Invites.create(ctx.server.id, ctx.owner.id)
      config = Application.get_env(:lumia, :instance, [])
      Application.put_env(:lumia, :instance, Keyword.put(config, :open_registration, false))
      on_exit(fn -> Application.put_env(:lumia, :instance, config) end)

      closed =
        post_proto(ctx.conn, "/api/v1/accounts", %V1.CreateAccountRequest{
          handle: "nobody#{System.unique_integer([:positive])}",
          password: "correct-horse-battery-staple"
        })

      assert closed.status == 403

      handle = "invited#{System.unique_integer([:positive])}"

      conn =
        post_proto(ctx.conn, "/api/v1/accounts", %V1.CreateAccountRequest{
          handle: handle,
          password: "correct-horse-battery-staple",
          invite_code: invite.code
        })

      assert conn.status == 201
      account = Lumia.Repo.get_by!(Lumia.Accounts.Account, handle: handle)
      assert Servers.member_for(ctx.server.id, account.id)
    end

    test "an unknown code makes no account", ctx do
      handle = "nobody#{System.unique_integer([:positive])}"

      conn =
        post_proto(ctx.conn, "/api/v1/accounts", %V1.CreateAccountRequest{
          handle: handle,
          password: "correct-horse-battery-staple",
          invite_code: "nope"
        })

      assert conn.status == 404
      refute Lumia.Repo.get_by(Lumia.Accounts.Account, handle: handle)
    end
  end

  describe "deleting" do
    test "the creator or a manager revokes; a member cannot", ctx do
      {:ok, invite} = Invites.create(ctx.server.id, ctx.owner.id)
      member = account("member")
      {:ok, _} = Servers.add_member(ctx.server.id, member.id, ctx.owner.id)

      assert delete(authed(ctx.conn, member), "/api/v1/invites/#{invite.code}").status == 403
      assert delete(authed(ctx.conn, ctx.owner), "/api/v1/invites/#{invite.code}").status == 204
      assert get(ctx.conn, "/api/v1/invites/#{invite.code}").status == 404
    end
  end
end
