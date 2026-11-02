defmodule LumiaWeb.Plugs.CorsTest do
  use LumiaWeb.ConnCase, async: true

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
