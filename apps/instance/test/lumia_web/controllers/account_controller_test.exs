defmodule LumiaWeb.AccountControllerTest do
  use LumiaWeb.ConnCase, async: true

  import Lumia.Fixtures

  alias Lumia.Auth
  alias Lumia.Servers
  alias Lumia.V1

  @proto "application/x-protobuf"

  defp authed(conn, account) do
    {:ok, _session, tokens} = Auth.create_session(account.id, Ecto.UUID.generate())

    conn
    |> put_req_header("authorization", "Bearer " <> tokens.access_token)
    |> put_req_header("accept", @proto)
  end

  defp profile(conn, viewer, account) do
    conn = conn |> authed(viewer) |> get("/api/v1/accounts/#{account.id}")
    {conn.status, conn.status == 200 && V1.Account.decode(conn.resp_body)}
  end

  test "a profile is visible to itself, a server mate, a DM partner and a request's recipient", %{
    conn: conn
  } do
    a = account("a")
    stranger = account("s")

    assert {404, _} = profile(conn, stranger, a)
    assert {200, %V1.Account{handle: handle}} = profile(conn, a, a)
    assert handle == a.handle

    mate = account("m")
    {:ok, server} = Servers.create_server(a.id, "Shared")
    {:ok, _} = Servers.add_member(server.id, mate.id, a.id)
    assert {200, %V1.Account{}} = profile(conn, mate, a)
    assert {200, %V1.Account{}} = profile(conn, a, mate)

    asked = account("r")
    {:ok, _request} = Lumia.Dms.send_request(a.id, asked.id)
    assert {200, %V1.Account{}} = profile(conn, asked, a)

    %{a: p, b: q} = personal_dm()
    assert {200, %V1.Account{}} = profile(conn, p, q)
    assert {404, _} = profile(conn, stranger, q)
  end

  test "a malformed id is not found", %{conn: conn} do
    conn = conn |> authed(account("a")) |> get("/api/v1/accounts/not-an-id")
    assert conn.status == 404
  end
end
