defmodule LumiaWeb.Plugs.CorsTest do
  use LumiaWeb.ConnCase, async: true

  alias LumiaWeb.Plugs.Cors

  @allowed "http://localhost:5173"
  @other "https://evil.example"

  test "a preflight from a trusted origin is allowed", %{conn: conn} do
    conn =
      conn
      |> put_req_header("origin", @allowed)
      |> put_req_header("access-control-request-method", "POST")
      |> options(~p"/api/v1/auth/sign-in/password")

    assert conn.status == 204
    assert get_resp_header(conn, "access-control-allow-origin") == [@allowed]
    assert [methods] = get_resp_header(conn, "access-control-allow-methods")
    assert methods =~ "POST"
    assert [headers] = get_resp_header(conn, "access-control-allow-headers")
    assert headers =~ "authorization"
    assert get_resp_header(conn, "vary") == ["origin"]
  end

  test "the desktop shell's origin is allowed for the API and the gateway", %{conn: conn} do
    conn =
      conn
      |> put_req_header("origin", "tauri://localhost")
      |> put_req_header("access-control-request-method", "GET")
      |> options(~p"/api/v1/account/snapshot")

    assert conn.status == 204
    assert get_resp_header(conn, "access-control-allow-origin") == ["tauri://localhost"]

    assert Cors.socket_origin?(URI.parse("tauri://localhost"))
    assert Cors.socket_origin?(URI.parse("https://tauri.localhost"))
    assert Cors.socket_origin?(URI.parse("http://localhost:5173"))
    assert Cors.socket_origin?(URI.parse("https://localhost:443"))
    refute Cors.socket_origin?(URI.parse("https://evil.example"))
  end

  test "a preflight from any other origin is refused", %{conn: conn} do
    conn =
      conn
      |> put_req_header("origin", @other)
      |> put_req_header("access-control-request-method", "POST")
      |> options(~p"/api/v1/auth/sign-in/password")

    assert conn.status == 403
    assert get_resp_header(conn, "access-control-allow-origin") == []
  end

  test "a request from a trusted origin carries the allow header", %{conn: conn} do
    conn =
      conn
      |> put_req_header("origin", @allowed)
      |> put_req_header("accept", "application/json")
      |> get(~p"/api/v1/instance")

    assert conn.status == 200
    assert get_resp_header(conn, "access-control-allow-origin") == [@allowed]
  end

  test "a request from another origin is answered without it", %{conn: conn} do
    conn =
      conn
      |> put_req_header("origin", @other)
      |> put_req_header("accept", "application/json")
      |> get(~p"/api/v1/instance")

    assert conn.status == 200
    assert get_resp_header(conn, "access-control-allow-origin") == []
  end

  test "a request without an origin is untouched", %{conn: conn} do
    conn = conn |> put_req_header("accept", "application/json") |> get(~p"/api/v1/instance")

    assert conn.status == 200
    assert get_resp_header(conn, "access-control-allow-origin") == []
    assert get_resp_header(conn, "vary") == []
  end
end
