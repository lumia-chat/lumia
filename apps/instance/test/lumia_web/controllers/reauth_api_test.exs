defmodule LumiaWeb.ReauthApiTest do
  @moduledoc """
  Reauth by every credential, and carried by a sign-in that proved one
  (specs/identity.md 10). OIDC's reauth is in `Lumia.Auth.OidcTest`.
  """

  use LumiaWeb.ConnCase, async: true

  import Lumia.Fixtures
  import Swoosh.TestAssertions

  alias Lumia.Accounts
  alias Lumia.Auth
  alias Lumia.Auth.Passkeys
  alias Lumia.Auth.ReauthCodes
  alias Lumia.Identity
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.V1

  @json "application/json"
  @password "correct-horse-battery-staple"

  defp post_proto(conn, path, struct) do
    conn
    |> put_req_header("content-type", @json)
    |> post(path, Lumia.ProtoJSON.encode!(struct))
  end

  defp decode(conn, module), do: Lumia.ProtoJSON.decode!(conn.resp_body, module)

  defp authed(conn, session_tokens),
    do: put_req_header(conn, "authorization", "Bearer " <> session_tokens.access_token)

  defp session(account, opts \\ []) do
    {:ok, session, tokens} = Auth.create_session(account.id, Ecto.UUID.generate(), opts)
    {session, tokens}
  end

  defp reauth(conn, tokens, req), do: post_proto(authed(conn, tokens), "/api/v1/auth/reauth", req)

  defp registered(name) do
    {:ok, account} =
      Accounts.register(%{
        handle: "#{name}#{System.unique_integer([:positive])}",
        email: "#{name}#{System.unique_integer([:positive])}@example.com",
        password: @password
      })

    account
  end

  describe "a sign-in that proved a credential carries a reauth token" do
    @tag failure_injection: [identity: 15]
    test "password sign-in does, and its refresh does not", %{conn: conn} do
      account = registered("pw")

      signed =
        post_proto(conn, "/api/v1/auth/sign-in/password", %V1.PasswordSignInRequest{
          handle_or_email: account.handle,
          password: @password
        })

      assert %V1.TokenPair{reauth_token: token, reauth_expires_at: %{}, refresh_token: refresh} =
               decode(signed, V1.TokenPair)

      assert {:ok, session} = Identity.verify_reauth(token)
      assert session.account_id == account.id
      # Single use.
      assert {:error, :unauthenticated} = Identity.verify_reauth(token)

      refreshed =
        post_proto(conn, "/api/v1/auth/refresh", %V1.RefreshRequest{refresh_token: refresh})

      assert %V1.TokenPair{reauth_token: ""} = decode(refreshed, V1.TokenPair)
    end

    @tag failure_injection: [identity: 15]
    test "account creation with a password does", %{conn: conn} do
      created =
        post_proto(conn, "/api/v1/accounts", %V1.CreateAccountRequest{
          handle: "new#{System.unique_integer([:positive])}",
          password: @password
        })

      assert created.status == 201
      assert %V1.TokenPair{reauth_token: token} = decode(created, V1.TokenPair)
      assert {:ok, _session} = Identity.verify_reauth(token)
    end

    @tag failure_injection: [identity: 15]
    test "a mailed magic link does", %{conn: conn} do
      account = registered("mail")
      {:ok, token} = Auth.MagicLink.mint(account)

      signed =
        post_proto(conn, "/api/v1/auth/sign-in/magic-link/complete", %V1.MagicLinkCompleteRequest{
          token: token,
          device_id: Proto.uuid(Ecto.UUID.generate())
        })

      assert %V1.TokenPair{reauth_token: reauth} = decode(signed, V1.TokenPair)
      assert {:ok, _session} = Identity.verify_reauth(reauth)
    end

    @tag failure_injection: [identity: 15]
    test "a handoff passes on a fresh credential, and only a fresh one", %{conn: conn} do
      account = registered("hand")

      complete = fn handoff_token ->
        conn
        |> post_proto("/api/v1/auth/sign-in/magic-link/complete", %V1.MagicLinkCompleteRequest{
          token: handoff_token,
          device_id: Proto.uuid(Ecto.UUID.generate())
        })
        |> decode(V1.TokenPair)
      end

      handoff = fn tokens ->
        conn |> authed(tokens) |> post("/api/v1/auth/handoff") |> decode(V1.HandoffResponse)
      end

      # A session that proved a credential a moment ago: the handoff carries it.
      {_fresh, fresh_tokens} = session(account, credentialed_at: DateTime.utc_now())
      %V1.HandoffResponse{token: t1} = handoff.(fresh_tokens)
      assert %V1.TokenPair{reauth_token: passed} = complete.(t1)
      assert {:ok, _} = Identity.verify_reauth(passed)

      # One that proved it six minutes ago, or never: nothing to pass on.
      stale_at = DateTime.add(DateTime.utc_now(), -360, :second)
      {_stale, stale_tokens} = session(account, credentialed_at: stale_at)
      %V1.HandoffResponse{token: t2} = handoff.(stale_tokens)
      assert %V1.TokenPair{reauth_token: ""} = complete.(t2)

      {_none, none_tokens} = session(account)
      %V1.HandoffResponse{token: t3} = handoff.(none_tokens)
      assert %V1.TokenPair{reauth_token: ""} = complete.(t3)
    end

    test "a handoff's session passes on the original time, so handoffs do not renew it",
         %{conn: conn} do
      account = registered("chain")
      at = DateTime.add(DateTime.utc_now(), -200, :second)
      {_first, first_tokens} = session(account, credentialed_at: at)

      %V1.HandoffResponse{token: token} =
        conn |> authed(first_tokens) |> post("/api/v1/auth/handoff") |> decode(V1.HandoffResponse)

      {:ok, second, _tokens} = Auth.MagicLink.complete(token, Ecto.UUID.generate())
      assert DateTime.compare(second.credentialed_at, at) == :eq
    end
  end

  describe "a mailed code" do
    @tag failure_injection: [identity: 14]
    test "is mailed, works once, and only for the session that asked", %{conn: conn} do
      account = registered("code")
      {_session, tokens} = session(account)
      {_other, other_tokens} = session(account)

      asked = conn |> authed(tokens) |> post("/api/v1/auth/reauth/magic-link")
      assert asked.status == 202
      code = code_from_mail(account.email)

      assert reauth(conn, other_tokens, %V1.ReauthRequest{credential: {:magic_code, code}}).status ==
               401

      ok = reauth(conn, tokens, %V1.ReauthRequest{credential: {:magic_code, code}})
      assert %V1.ReauthResponse{reauth_token: token} = decode(ok, V1.ReauthResponse)
      assert {:ok, _} = Identity.verify_reauth(token)

      assert reauth(conn, tokens, %V1.ReauthRequest{credential: {:magic_code, code}}).status ==
               401
    end

    @tag failure_injection: [identity: 14]
    test "is spent by five wrong attempts", %{conn: conn} do
      account = registered("guess")
      {_session, tokens} = session(account)

      assert conn |> authed(tokens) |> post("/api/v1/auth/reauth/magic-link") |> Map.get(:status) ==
               202

      code = code_from_mail(account.email)
      wrong = if code == "000000", do: "000001", else: "000000"

      for _ <- 1..5 do
        assert reauth(conn, tokens, %V1.ReauthRequest{credential: {:magic_code, wrong}}).status ==
                 401
      end

      assert reauth(conn, tokens, %V1.ReauthRequest{credential: {:magic_code, code}}).status ==
               401

      assert Repo.aggregate(ReauthCodes.Code, :count) == 0
    end

    test "needs an address, and five an hour at most", %{conn: conn} do
      {_session, tokens} = session(account("noaddr"))
      none = conn |> authed(tokens) |> post("/api/v1/auth/reauth/magic-link")
      assert none.status == 400

      assert %{"code" => "FAILED_PRECONDITION", "details" => %{"state" => "no_email"}} =
               Jason.decode!(none.resp_body)

      account = registered("many")
      {_session, tokens} = session(account)

      for _ <- 1..5,
          do:
            assert(
              conn |> authed(tokens) |> post("/api/v1/auth/reauth/magic-link") |> Map.get(:status) ==
                202
            )

      assert (conn |> authed(tokens) |> post("/api/v1/auth/reauth/magic-link")).status == 429
    end
  end

  describe "a passkey" do
    @tag failure_injection: [identity: 14]
    test "of this account reauths; another account's, or a bad one, does not", %{conn: conn} do
      account = registered("pk")
      other = registered("pkother")
      key = passkey(account)
      other_key = passkey(other)
      {_session, tokens} = session(account)

      {challenge_id, challenge} = assert_start(conn, account.handle)

      ok =
        reauth(conn, tokens, %V1.ReauthRequest{
          credential: {:passkey_assertion, assertion(key, challenge, 1)},
          passkey_challenge_id: challenge_id
        })

      assert %V1.ReauthResponse{reauth_token: token} = decode(ok, V1.ReauthResponse)
      assert {:ok, _} = Identity.verify_reauth(token)

      {challenge_id, challenge} = assert_start(conn, other.handle)

      assert reauth(conn, tokens, %V1.ReauthRequest{
               credential: {:passkey_assertion, assertion(other_key, challenge, 1)},
               passkey_challenge_id: challenge_id
             }).status == 401

      {challenge_id, _challenge} = assert_start(conn, account.handle)

      assert reauth(conn, tokens, %V1.ReauthRequest{
               credential: {:passkey_assertion, "{\"rawId\":\"AAAA\"}"},
               passkey_challenge_id: challenge_id
             }).status == 401
    end

    @tag failure_injection: [identity: 15]
    test "sign-in by passkey carries a reauth token", %{conn: conn} do
      account = registered("pksign")
      key = passkey(account)
      {challenge_id, challenge} = assert_start(conn, account.handle)

      signed =
        post_proto(conn, "/api/v1/auth/passkey/assert/finish", %V1.PasskeyAssertFinishRequest{
          challenge_id: challenge_id,
          credential_json: assertion(key, challenge, 1),
          device_id: Proto.uuid(Ecto.UUID.generate())
        })

      assert %V1.TokenPair{reauth_token: token} = decode(signed, V1.TokenPair)
      assert {:ok, _} = Identity.verify_reauth(token)
    end
  end

  @tag failure_injection: [identity: 14]
  test "a credential the account does not have is refused", %{conn: conn} do
    {_session, tokens} = session(account("bare"))

    assert reauth(conn, tokens, %V1.ReauthRequest{credential: {:password, "anything"}}).status ==
             401

    assert reauth(conn, tokens, %V1.ReauthRequest{}).status == 400
  end

  # ---- Helpers ---------------------------------------------------------------------

  defp code_from_mail(email) do
    assert_email_sent(fn mail ->
      assert {_, ^email} = hd(mail.to)
      [code] = Regex.run(~r/^\d{6}$/m, mail.text_body)
      send(self(), {:code, code})
    end)

    assert_receive {:code, code}
    code
  end

  defp assert_start(conn, handle) do
    %V1.PasskeyChallenge{challenge_id: id, options_json: options} =
      conn
      |> post_proto("/api/v1/auth/passkey/assert/start", %V1.PasskeyAssertStartRequest{
        handle_or_email: handle
      })
      |> decode(V1.PasskeyChallenge)

    {id, options |> Jason.decode!() |> Map.fetch!("challenge")}
  end

  # A P-256 passkey stored for the account as registration would store it:
  # its COSE key, as Wax decodes one.
  defp passkey(account) do
    {public, private} = :crypto.generate_key(:ecdh, :secp256r1)
    <<4, x::binary-size(32), y::binary-size(32)>> = public
    credential_id = :crypto.strong_rand_bytes(16)
    cose = %{1 => 2, 3 => -7, -1 => 1, -2 => x, -3 => y}

    {:ok, _} =
      Repo.insert(%Passkeys.Passkey{
        account_id: account.id,
        credential_id: credential_id,
        public_key: :erlang.term_to_binary(cose),
        label: "Test"
      })

    %{id: credential_id, private: private}
  end

  # The JSON `PublicKeyCredential.toJSON()` gives for an assertion: the
  # authenticator data for this instance's relying party with the user
  # present, and an ECDSA signature over it and the client data's hash.
  defp assertion(key, challenge, sign_count) do
    rp_hash = :crypto.hash(:sha256, Identity.host())
    auth_data = rp_hash <> <<0x01, sign_count::32>>

    client_data =
      Jason.encode!(%{
        type: "webauthn.get",
        challenge: challenge,
        origin: LumiaWeb.Endpoint.url()
      })

    signature =
      :crypto.sign(
        :ecdsa,
        :sha256,
        auth_data <> :crypto.hash(:sha256, client_data),
        [key.private, :secp256r1]
      )

    b64 = &Base.url_encode64(&1, padding: false)

    Jason.encode!(%{
      id: b64.(key.id),
      rawId: b64.(key.id),
      type: "public-key",
      response: %{
        authenticatorData: b64.(auth_data),
        clientDataJSON: b64.(client_data),
        signature: b64.(signature)
      }
    })
  end
end
