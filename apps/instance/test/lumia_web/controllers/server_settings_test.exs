defmodule LumiaWeb.ServerSettingsTest do
  @moduledoc "A server's settings (architecture 6.7): name, picture and retention, and who hears of a change."

  use LumiaWeb.ConnCase, async: false
  use Oban.Testing, repo: Lumia.Repo

  import Ecto.Query
  import Lumia.Fixtures

  alias Lumia.Accounts
  alias Lumia.Auth
  alias Lumia.Servers
  alias Lumia.V1
  alias Lumia.Workers.ServerUpdated

  @proto "application/x-protobuf"
  @photo File.read!(Path.expand("../../support/fixtures/media/sideways.jpg", __DIR__))

  defp authed(conn, account) do
    {:ok, _session, tokens} = Auth.create_session(account.id, Ecto.UUID.generate())

    conn
    |> put_req_header("authorization", "Bearer " <> tokens.access_token)
    |> put_req_header("accept", @proto)
  end

  defp patch_server(conn, account, server, %V1.UpdateServerRequest{} = req) do
    conn
    |> authed(account)
    |> put_req_header("content-type", @proto)
    |> patch("/api/v1/servers/#{server.id}", V1.UpdateServerRequest.encode(req))
  end

  defp put_icon(conn, account, server, bytes) do
    conn
    |> authed(account)
    |> put_req_header("content-type", "image/jpeg")
    |> put("/api/v1/servers/#{server.id}/icon", bytes)
  end

  defp error(conn), do: V1.Error.decode(conn.resp_body)

  # The server_updated events on an account's log, as the servers they carry.
  defp told(account) do
    for %V1.AccountEvent{payload: {:server_updated, %V1.ServerUpdated{server: server}}} <-
          Accounts.events_after(account.id, 0),
        do: server
  end

  defp run_fan_out do
    for %Oban.Job{args: args} <- all_enqueued(worker: ServerUpdated) do
      assert :ok = perform_job(ServerUpdated, args)
    end
  end

  setup do
    owner = account("owner")
    member = account("member")
    {:ok, server} = Servers.create_server(owner.id, "Acme")
    {:ok, _} = Servers.add_member(server.id, member.id, owner.id)
    %{owner: owner, member: member, server: server}
  end

  describe "the name" do
    test "the owner renames the server, and every member is told", ctx do
      renamed =
        patch_server(ctx.conn, ctx.owner, ctx.server, %V1.UpdateServerRequest{name: "  Acme Co  "})

      assert renamed.status == 200
      assert %V1.Server{name: "Acme Co"} = V1.Server.decode(renamed.resp_body)

      # Told by a job after the commit, not inside the request.
      assert told(ctx.member) == []
      run_fan_out()
      assert [%V1.Server{name: "Acme Co"}] = told(ctx.member)
      assert [%V1.Server{name: "Acme Co"}] = told(ctx.owner)

      # Someone in another server hears nothing.
      elsewhere = account("elsewhere")
      {:ok, _} = Servers.create_server(elsewhere.id, "Other")
      assert told(elsewhere) == []
    end

    test "needs MANAGE_SERVER: a member without it is refused, one given it may", ctx do
      refused =
        patch_server(ctx.conn, ctx.member, ctx.server, %V1.UpdateServerRequest{name: "Mine"})

      assert refused.status == 403
      assert %V1.Error{code: :PERMISSION_DENIED} = error(refused)

      {:ok, role} =
        Servers.create_role(ctx.server.id, ctx.owner.id, %{
          name: "admins",
          permissions: [:manage_server]
        })

      :ok = Servers.assign_role(ctx.server.id, ctx.member.id, role.id, ctx.owner.id)

      allowed =
        patch_server(ctx.conn, ctx.member, ctx.server, %V1.UpdateServerRequest{name: "Ours"})

      assert allowed.status == 200

      # Someone not in the server does not learn it exists.
      stranger = account("stranger")

      hidden =
        patch_server(ctx.conn, stranger, ctx.server, %V1.UpdateServerRequest{name: "Theirs"})

      assert hidden.status == 404
    end

    test "is 1 to 100 characters once trimmed, at creation too", ctx do
      for name <- ["", "   ", String.duplicate("a", 101)] do
        bad = patch_server(ctx.conn, ctx.owner, ctx.server, %V1.UpdateServerRequest{name: name})
        assert bad.status == 400
        assert %V1.Error{code: :INVALID_ARGUMENT, details: %{"field" => "name"}} = error(bad)
      end

      assert {:error, {:invalid_argument, "name"}} = Servers.create_server(ctx.owner.id, "  ")

      created =
        ctx.conn
        |> authed(ctx.owner)
        |> put_req_header("content-type", @proto)
        |> post(
          "/api/v1/servers",
          V1.CreateServerRequest.encode(%V1.CreateServerRequest{name: " "})
        )

      assert %V1.Error{details: %{"field" => "name"}} = error(created)

      # A field left out is left as it was.
      kept =
        patch_server(ctx.conn, ctx.owner, ctx.server, %V1.UpdateServerRequest{retention_days: 30})

      assert %V1.Server{name: "Acme", retention_days: 30} = V1.Server.decode(kept.resp_body)
    end
  end

  describe "retention" do
    test "is set in days and 0 keeps messages for ever, each change audited", ctx do
      set =
        patch_server(ctx.conn, ctx.owner, ctx.server, %V1.UpdateServerRequest{retention_days: 90})

      assert %V1.Server{retention_days: 90} = V1.Server.decode(set.resp_body)
      assert Lumia.Repo.get!(Lumia.Servers.Server, ctx.server.id).retention_days == 90

      forever =
        patch_server(ctx.conn, ctx.owner, ctx.server, %V1.UpdateServerRequest{retention_days: 0})

      assert %V1.Server{retention_days: 0} = V1.Server.decode(forever.resp_body)
      assert Lumia.Repo.get!(Lumia.Servers.Server, ctx.server.id).retention_days == nil

      _ = patch_server(ctx.conn, ctx.owner, ctx.server, %V1.UpdateServerRequest{name: "Renamed"})

      actions =
        Lumia.Repo.all(
          from a in Lumia.Audit.Entry,
            where:
              a.scope_id == ^ctx.server.id and a.action in ["server.retention", "server.name"],
            select: a.action
        )

      assert Enum.sort(actions) == ["server.name", "server.retention", "server.retention"]
    end

    test "nothing changed is nothing to tell", ctx do
      _ = patch_server(ctx.conn, ctx.owner, ctx.server, %V1.UpdateServerRequest{name: "Acme"})
      assert all_enqueued(worker: ServerUpdated) == []
    end
  end

  describe "the picture" do
    test "is put as a picture's bytes, served to anyone, replaced and removed", ctx do
      put = put_icon(ctx.conn, ctx.owner, ctx.server, @photo)
      assert put.status == 200

      assert %V1.Server{icon_url: "/api/v1/avatars/" <> _ = first} =
               V1.Server.decode(put.resp_body)

      got = get(build_conn(), first)
      assert got.status == 200
      assert <<0xFF, 0xD8, _::binary>> = got.resp_body

      run_fan_out()
      assert [%V1.Server{icon_url: ^first}] = told(ctx.member)

      # Replaced: the first picture's blob goes.
      again = put_icon(ctx.conn, ctx.owner, ctx.server, @photo)
      assert %V1.Server{icon_url: second} = V1.Server.decode(again.resp_body)
      assert second != first
      assert get(build_conn(), first).status == 404

      gone = ctx.conn |> authed(ctx.owner) |> delete("/api/v1/servers/#{ctx.server.id}/icon")
      assert %V1.Server{icon_url: ""} = V1.Server.decode(gone.resp_body)
      assert get(build_conn(), second).status == 404
    end

    test "needs MANAGE_SERVER, and what is not a picture is refused in words", ctx do
      refused = put_icon(ctx.conn, ctx.member, ctx.server, @photo)
      assert refused.status == 403

      not_one = put_icon(ctx.conn, ctx.owner, ctx.server, "<svg onload=alert(1)/>")

      assert %V1.Error{details: %{"field" => "picture", "reason" => "unsupported"}} =
               error(not_one)

      removed = ctx.conn |> authed(ctx.member) |> delete("/api/v1/servers/#{ctx.server.id}/icon")
      assert removed.status == 403
    end
  end

  describe "the snapshot" do
    test "carries the caller's own server-level permissions, so a client knows what to offer",
         ctx do
      permissions = fn account ->
        snapshot = Lumia.Snapshots.account(account.id)

        [membership] =
          Enum.filter(snapshot.memberships, &(&1.server_id == Lumia.Proto.uuid(ctx.server.id)))

        membership.permissions
      end

      assert :MANAGE_SERVER in permissions.(ctx.owner)
      assert :ADMINISTRATOR in permissions.(ctx.owner)
      refute :MANAGE_SERVER in permissions.(ctx.member)
      assert :SEND_MESSAGES in permissions.(ctx.member)

      {:ok, role} =
        Servers.create_role(ctx.server.id, ctx.owner.id, %{
          name: "admins",
          permissions: [:manage_server]
        })

      :ok = Servers.assign_role(ctx.server.id, ctx.member.id, role.id, ctx.owner.id)
      assert :MANAGE_SERVER in permissions.(ctx.member)
    end
  end
end
