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

  describe "PATCH /account" do
    defp patch_account(conn, account, %V1.UpdateAccountRequest{} = req) do
      conn =
        conn
        |> authed(account)
        |> put_req_header("content-type", @proto)
        |> patch("/api/v1/account", V1.UpdateAccountRequest.encode(req))

      {conn.status, conn.resp_body}
    end

    test "changes the handle, the display name, or both, and leaves what is not named", %{
      conn: conn
    } do
      a = account("a")

      assert {200, body} = patch_account(conn, a, %V1.UpdateAccountRequest{display_name: "Ada"})
      assert %V1.Account{display_name: "Ada", handle: handle} = V1.Account.decode(body)
      assert handle == a.handle

      assert {200, body} = patch_account(conn, a, %V1.UpdateAccountRequest{handle: "ada_l"})
      assert %V1.Account{display_name: "Ada", handle: "ada_l"} = V1.Account.decode(body)

      req = %V1.UpdateAccountRequest{handle: "countess", display_name: ""}
      assert {200, body} = patch_account(conn, a, req)
      assert %V1.Account{display_name: "", handle: "countess"} = V1.Account.decode(body)
    end

    test "a taken handle is ALREADY_EXISTS and a bad one INVALID_ARGUMENT", %{conn: conn} do
      a = account("a")
      b = account("b")

      assert {409, body} = patch_account(conn, a, %V1.UpdateAccountRequest{handle: b.handle})
      assert %V1.Error{code: :ALREADY_EXISTS} = V1.Error.decode(body)
      assert {400, body} = patch_account(conn, a, %V1.UpdateAccountRequest{handle: "not one"})
      assert %V1.Error{code: :INVALID_ARGUMENT} = V1.Error.decode(body)
    end

    test "takes JSON as it takes protobuf, and needs a session", %{conn: conn} do
      a = account("a")

      json =
        conn
        |> authed(a)
        |> put_req_header("accept", "application/json")
        |> put_req_header("content-type", "application/json")
        |> patch("/api/v1/account", Jason.encode!(%{displayName: "Ada"}))

      assert %{"displayName" => "Ada", "handle" => handle} = json_response(json, 200)
      assert handle == a.handle

      anonymous =
        conn
        |> put_req_header("content-type", "application/json")
        |> patch("/api/v1/account", Jason.encode!(%{displayName: "Nobody"}))

      assert anonymous.status == 401
    end
  end

  test "a malformed id is not found", %{conn: conn} do
    conn = conn |> authed(account("a")) |> get("/api/v1/accounts/not-an-id")
    assert conn.status == 404
  end
end
