defmodule LumiaWeb.PasskeyApiTest do
  use LumiaWeb.ConnCase, async: true

  import Lumia.Fixtures

  alias Lumia.Auth
  alias Lumia.Auth.Passkeys
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

  test "registration starts with creation options for the account", %{conn: conn} do
    account = account("pk")
    conn = post(authed(conn, account), "/api/v1/auth/passkey/register/start")

    assert %V1.PasskeyChallenge{challenge_id: id, options_json: json} =
             decode(conn, V1.PasskeyChallenge)

    assert {:ok, _} = Ecto.UUID.cast(id)
    options = Jason.decode!(json)
    assert options["rp"]["id"] == Lumia.Identity.host()
    assert options["user"]["name"] == account.handle
    assert options["attestation"] == "none"
    assert String.length(options["challenge"]) >= 32
  end

  test "a credential that is not one is refused and the challenge is spent", %{conn: conn} do
    account = account("pk")
    started = post(authed(conn, account), "/api/v1/auth/passkey/register/start")
    %V1.PasskeyChallenge{challenge_id: id} = decode(started, V1.PasskeyChallenge)

    finish = %V1.PasskeyRegisterFinishRequest{
      challenge_id: id,
      credential_json: "{}",
      label: "Key"
    }

    assert post_proto(authed(conn, account), "/api/v1/auth/passkey/register/finish", finish).status ==
             400

    # The challenge is single use, whatever came of it.
    assert post_proto(authed(conn, account), "/api/v1/auth/passkey/register/finish", finish).status ==
             401

    assert %V1.ListPasskeysResponse{passkeys: []} =
             decode(get(authed(conn, account), "/api/v1/auth/passkeys"), V1.ListPasskeysResponse)
  end

  test "sign-in starts in public, names the account's passkeys for a handle, and refuses a bad assertion",
       %{conn: conn} do
    conn1 =
      post_proto(conn, "/api/v1/auth/passkey/assert/start", %V1.PasskeyAssertStartRequest{
        handle_or_email: "nobody-here"
      })

    assert %V1.PasskeyChallenge{challenge_id: id, options_json: json} =
             decode(conn1, V1.PasskeyChallenge)

    assert Jason.decode!(json)["allowCredentials"] == []
    assert Jason.decode!(json)["rpId"] == Lumia.Identity.host()

    bad = %V1.PasskeyAssertFinishRequest{
      challenge_id: id,
      credential_json: "{\"rawId\":\"AAAA\"}"
    }

    assert post_proto(conn, "/api/v1/auth/passkey/assert/finish", bad).status == 401

    assert post_proto(conn, "/api/v1/auth/passkey/assert/finish", %{
             bad
             | challenge_id: Ecto.UUID.generate()
           }).status == 401
  end

  test "a passkey is listed and removed", %{conn: conn} do
    account = account("pk")

    {:ok, passkey} =
      Lumia.Repo.insert(%Passkeys.Passkey{
        account_id: account.id,
        credential_id: :crypto.strong_rand_bytes(16),
        public_key: :erlang.term_to_binary(%{}),
        label: "Laptop"
      })

    assert %V1.ListPasskeysResponse{passkeys: [%V1.Passkey{label: "Laptop"}]} =
             decode(get(authed(conn, account), "/api/v1/auth/passkeys"), V1.ListPasskeysResponse)

    other = account("other")
    assert delete(authed(conn, other), "/api/v1/auth/passkeys/#{passkey.id}").status == 404
    assert delete(authed(conn, account), "/api/v1/auth/passkeys/#{passkey.id}").status == 204

    assert %V1.ListPasskeysResponse{passkeys: []} =
             decode(get(authed(conn, account), "/api/v1/auth/passkeys"), V1.ListPasskeysResponse)
  end
end
