defmodule LumiaWeb.HandoffApiTest do
  use LumiaWeb.ConnCase, async: true

  import Lumia.Fixtures

  alias Lumia.Auth
  alias Lumia.Proto
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

  test "a handoff token signs another device in once, as a magic-link token does", %{conn: conn} do
    account = account("handoff")
    minted = post(authed(conn, account), "/api/v1/auth/handoff")
    assert %V1.HandoffResponse{token: token, expires_at: expires_at} = decode(minted, V1.HandoffResponse)
    assert byte_size(token) >= 32
    assert DateTime.compare(Proto.from_timestamp(expires_at), DateTime.utc_now()) == :gt

    complete = %V1.MagicLinkCompleteRequest{token: token, device_id: Proto.uuid(Ecto.UUID.generate())}
    signed = post_proto(build_conn(), "/api/v1/auth/sign-in/magic-link/complete", complete)
    assert %V1.TokenPair{access_token: access} = decode(signed, V1.TokenPair)
    assert {:ok, session} = Auth.verify_access(access)
    assert session.account_id == account.id

    again = post_proto(build_conn(), "/api/v1/auth/sign-in/magic-link/complete", complete)
    assert again.status == 401
  end

  test "a handoff needs a session", %{conn: conn} do
    assert post(conn, "/api/v1/auth/handoff").status == 401
  end
end
