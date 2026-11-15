defmodule LumiaWeb.IdentityApiTest do
  use LumiaWeb.ConnCase, async: true

  import Lumia.IdentityFixtures

  alias Lumia.Accounts
  alias Lumia.Crypto
  alias Lumia.Identity
  alias Lumia.Proto
  alias Lumia.V1

  @proto "application/x-protobuf"

  defp post_proto(conn, path, struct) do
    conn
    |> put_req_header("content-type", @proto)
    |> put_req_header("accept", @proto)
    |> post(path, struct.__struct__.encode(struct))
  end

  defp challenge(conn, purpose) do
    resp = post_proto(conn, "/api/v1/auth/challenge", %V1.ChallengeRequest{purpose: purpose})
    %V1.ChallengeResponse{challenge: bytes} = V1.ChallengeResponse.decode(resp.resp_body)
    bytes
  end

  defp proof(conn, identity, device, roster, purpose) do
    bytes = challenge(conn, purpose)

    %V1.IdentityProof{
      challenge: bytes,
      identity_id: identity.identity_id,
      device_id: device.id,
      signature: %V1.Signature{
        signer_kind: :DEVICE,
        signer_id: device.id,
        signature: Crypto.sign(bytes, device.priv)
      },
      roster: roster
    }
  end

  defp signed_profile(identity, device, version, display_name) do
    body =
      V1.ProfileBody.encode(%V1.ProfileBody{
        identity_id: identity.identity_id,
        version: version,
        display_name: display_name,
        pronouns: "they/them",
        bio: "Here for the #{display_name} channel.",
        issued_at: Proto.timestamp(DateTime.utc_now())
      })

    %V1.SignedProfile{
      body: body,
      signature: %V1.Signature{
        signer_kind: :DEVICE,
        signer_id: device.id,
        signature: Crypto.sign(body, device.priv)
      }
    }
  end

  test "a signed profile is imported at creation and replaced by a newer one", %{conn: conn} do
    %{identity: identity, device: phone, roster: v1} = bootstrap()
    handle = "pat#{System.unique_integer([:positive])}"

    created =
      post_proto(conn, "/api/v1/accounts/by-identity", %V1.CreateAccountRequest{
        handle: handle,
        proof: proof(conn, identity, phone, v1, :CREATE_ACCOUNT),
        profile: signed_profile(identity, phone, 1, "Pat Doe")
      })

    assert created.status == 201
    %V1.TokenPair{access_token: access} = V1.TokenPair.decode(created.resp_body)
    account = Lumia.Repo.get_by!(Lumia.Accounts.Account, handle: handle)
    assert account.display_name == "Pat Doe"
    # Pronouns and bio come out of the verified profile as the display name does.
    assert account.pronouns == "they/them"
    assert account.bio == "Here for the Pat Doe channel."

    authed = put_req_header(conn, "authorization", "Bearer " <> access)
    snap = get(authed, "/api/v1/account/snapshot")

    assert %V1.AccountSnapshot{
             account: %V1.Account{
               display_name: "Pat Doe",
               pronouns: "they/them",
               bio: "Here for the Pat Doe channel.",
               profile: %V1.SignedProfile{}
             }
           } =
             Lumia.ProtoJSON.decode!(snap.resp_body, V1.AccountSnapshot)

    # A newer version replaces it; an older one or a stranger's signature does not.
    newer = signed_profile(identity, phone, 2, "Pat D.")

    put =
      authed
      |> put_req_header("content-type", @proto)
      |> put("/api/v1/account/profile", V1.SignedProfile.encode(newer))

    assert put.status == 200
    assert Lumia.Repo.get!(Lumia.Accounts.Account, account.id).display_name == "Pat D."

    stale =
      authed
      |> put_req_header("content-type", @proto)
      |> put(
        "/api/v1/account/profile",
        V1.SignedProfile.encode(signed_profile(identity, phone, 1, "Old"))
      )

    assert stale.status == 409

    stranger = device("stranger")

    forged =
      authed
      |> put_req_header("content-type", @proto)
      |> put(
        "/api/v1/account/profile",
        V1.SignedProfile.encode(signed_profile(identity, stranger, 3, "Forged"))
      )

    assert forged.status == 401
  end

  test "a profile the roster does not vouch for refuses the creation", %{conn: conn} do
    %{identity: identity, device: phone, roster: v1} = bootstrap()
    stranger = device("stranger")

    refused =
      post_proto(conn, "/api/v1/accounts/by-identity", %V1.CreateAccountRequest{
        handle: "nope#{System.unique_integer([:positive])}",
        proof: proof(conn, identity, phone, v1, :CREATE_ACCOUNT),
        profile: signed_profile(identity, stranger, 1, "Forged")
      })

    assert refused.status == 401
  end

  @tag failure_injection: [identity: 15]
  test "create an account by identity proof, then sign in by proof, then publish a roster", %{
    conn: conn
  } do
    %{identity: identity, device: phone, roster: v1} = bootstrap()

    created =
      post_proto(conn, "/api/v1/accounts/by-identity", %V1.CreateAccountRequest{
        handle: "ryan#{System.unique_integer([:positive])}",
        proof: proof(conn, identity, phone, v1, :CREATE_ACCOUNT)
      })

    assert created.status == 201
    # Identity proof is not a credential: no reauth token (identity spec 10).
    assert %V1.TokenPair{reauth_token: ""} = V1.TokenPair.decode(created.resp_body)

    # A second account for the same identity is refused.
    again =
      post_proto(conn, "/api/v1/accounts/by-identity", %V1.CreateAccountRequest{
        handle: "other#{System.unique_integer([:positive])}",
        proof: proof(conn, identity, phone, v1, :CREATE_ACCOUNT)
      })

    assert again.status == 409

    signed =
      post_proto(conn, "/api/v1/auth/sign-in/identity", %V1.IdentityProofSignInRequest{
        proof: proof(conn, identity, phone, v1, :SIGN_IN)
      })

    assert %V1.TokenPair{access_token: access, reauth_token: ""} =
             V1.TokenPair.decode(signed.resp_body)

    laptop = device("laptop")

    v2 =
      roster(identity, phone,
        version: 2,
        devices: [cert(identity, phone), cert(identity, laptop, phone, added_at: now(1))]
      )

    published =
      conn
      |> put_req_header("authorization", "Bearer " <> access)
      |> post_proto("/api/v1/identity/roster", %V1.PublishRosterRequest{roster: v2})

    assert published.status == 204
    assert Identity.held_roster(identity.identity_id).version == 2

    # Presenting v1 again is ignored; the answer carries v2 for the client
    # to adopt (identity spec 5, "older roster"; protocol section 9).
    stale =
      conn
      |> put_req_header("authorization", "Bearer " <> access)
      |> post_proto("/api/v1/identity/roster", %V1.PublishRosterRequest{roster: v1})

    assert stale.status == 409

    assert %V1.Error{code: :CONFLICT, details: %{"held" => held}} =
             V1.Error.decode(stale.resp_body)

    assert %V1.SignedRoster{} = held_roster = held |> Base.decode64!() |> V1.SignedRoster.decode()
    assert V1.RosterBody.decode(held_roster.body).version == 2

    # The same at sign-in: a device carrying a stale roster learns the held one.
    stale_sign_in =
      post_proto(conn, "/api/v1/auth/sign-in/identity", %V1.IdentityProofSignInRequest{
        proof: proof(conn, identity, phone, v1, :SIGN_IN)
      })

    assert stale_sign_in.status == 409
    assert %V1.Error{details: %{"held" => ^held}} = V1.Error.decode(stale_sign_in.resp_body)

    fetched =
      conn
      |> put_req_header("authorization", "Bearer " <> access)
      |> put_req_header("accept", @proto)
      |> get("/api/v1/identity/roster")

    assert %V1.GetRosterResponse{roster: %V1.SignedRoster{}} =
             V1.GetRosterResponse.decode(fetched.resp_body)
  end

  @tag failure_injection: [identity: 1, identity: 2]
  test "revoking a device needs reauth; the revoked device's token dies", %{conn: conn} do
    %{identity: identity, device: phone, roster: v1} = bootstrap()

    {:ok, account} =
      Accounts.register(%{
        handle: "r#{System.unique_integer([:positive])}",
        password: "correct horse battery",
        identity_id: identity.identity_id
      })

    {:ok, _} = Identity.accept_roster(v1, :publish)

    laptop = device("laptop")

    v2 =
      roster(identity, phone,
        version: 2,
        devices: [cert(identity, phone), cert(identity, laptop, phone, added_at: now(1))]
      )

    {:ok, _} = Identity.accept_roster(v2, :publish)

    {:ok, _, phone_tokens} = Lumia.Auth.create_session(account.id, Proto.from_uuid(phone.id))
    {:ok, _, laptop_tokens} = Lumia.Auth.create_session(account.id, Proto.from_uuid(laptop.id))
    authed = put_req_header(conn, "authorization", "Bearer " <> phone_tokens.access_token)

    v3 =
      roster(identity, phone,
        version: 3,
        devices: [cert(identity, phone)],
        revoked: [revocation(identity, laptop, phone)]
      )

    refused = post_proto(authed, "/api/v1/identity/roster", %V1.PublishRosterRequest{roster: v3})
    assert refused.status == 403

    assert %V1.Error{code: :POLICY_DENIED, details: %{"reason" => "reauth_required"}} =
             V1.Error.decode(refused.resp_body)

    reauth =
      post_proto(authed, "/api/v1/auth/reauth", %V1.ReauthRequest{
        credential: {:password, "correct horse battery"}
      })

    assert %V1.ReauthResponse{reauth_token: token} = V1.ReauthResponse.decode(reauth.resp_body)

    revoked =
      post_proto(authed, "/api/v1/identity/devices/revoke", %V1.RevokeDeviceRequest{
        roster: v3,
        reauth_token: token
      })

    assert revoked.status == 204

    dead =
      conn
      |> put_req_header("authorization", "Bearer " <> laptop_tokens.access_token)
      |> get("/api/v1/account/snapshot")

    assert dead.status == 401
  end

  describe "starting over (identity spec 4.7)" do
    defp started(conn) do
      %{identity: old, device: old_phone, roster: old_v1} = bootstrap()

      {:ok, account} =
        Accounts.register(%{
          handle: "over#{System.unique_integer([:positive])}",
          password: "correct horse battery",
          identity_id: old.identity_id
        })

      {:ok, _} = Identity.accept_roster(old_v1, :publish)
      old_device_id = Proto.from_uuid(old_phone.id)
      {:ok, _, old_tokens} = Lumia.Auth.create_session(account.id, old_device_id)
      {:ok, _} = Lumia.Mls.publish_key_packages(account.id, old_device_id, ["kp"], "last")

      # The person, on a new device with neither the old phone nor the
      # phrase, signs in by password: no device of either identity yet.
      {:ok, _, tokens} = Lumia.Auth.create_session(account.id, Ecto.UUID.generate())
      authed = put_req_header(conn, "authorization", "Bearer " <> tokens.access_token)

      %{
        account: account,
        old: old,
        old_tokens: old_tokens,
        old_device_id: old_device_id,
        authed: authed
      }
    end

    defp reauth_token(authed) do
      authed
      |> post_proto("/api/v1/auth/reauth", %V1.ReauthRequest{
        credential: {:password, "correct horse battery"}
      })
      |> Map.get(:resp_body)
      |> V1.ReauthResponse.decode()
      |> Map.get(:reauth_token)
    end

    defp rebind(conn, authed, reauth, %{identity: identity, device: device, roster: roster}) do
      post_proto(authed, "/api/v1/identity/rebind", %V1.RebindIdentityRequest{
        proof: proof(conn, identity, device, roster, :LINK_ACCOUNT),
        reauth_token: reauth
      })
    end

    @tag failure_injection: [identity: 16]
    test "binds the account to the new identity, ends the old one's sessions and packages, and tells contacts",
         %{conn: conn} do
      s = started(conn)
      contact = Lumia.Fixtures.account("contact")
      Lumia.Fixtures.personal_dm(s.account, contact)
      new = bootstrap()

      assert rebind(conn, s.authed, reauth_token(s.authed), new).status == 204

      assert Lumia.Repo.get!(Accounts.Account, s.account.id).identity_id ==
               new.identity.identity_id

      dead =
        conn
        |> put_req_header("authorization", "Bearer " <> s.old_tokens.access_token)
        |> get("/api/v1/account/snapshot")

      assert dead.status == 401
      assert Lumia.Mls.key_package_count(s.account.id, s.old_device_id) == 0

      # The calling session, on a device neither roster names, lives on.
      assert get(s.authed, "/api/v1/account/snapshot").status == 200

      own = Accounts.events_after(s.account.id, 0) |> List.last()

      assert {:identity_rebound,
              %V1.IdentityRebound{old_identity_id: old_id, identity_id: new_id}} = own.payload

      assert old_id == s.old.identity_id
      assert new_id == new.identity.identity_id

      theirs =
        Accounts.events_after(contact.id, 0)
        |> Enum.find(&match?({:contact_identity_changed, _}, &1.payload))

      assert {:contact_identity_changed,
              %V1.ContactIdentityChanged{account_id: account_id, identity_id: ^new_id}} =
               theirs.payload

      assert Proto.from_uuid(account_id) == s.account.id
    end

    @tag failure_injection: [identity: 16]
    test "an identity bound to another account here is refused, and so is no reauth", %{
      conn: conn
    } do
      s = started(conn)
      taken = bootstrap()

      {:ok, _} =
        Accounts.register(%{
          handle: "taken#{System.unique_integer([:positive])}",
          password: "correct horse battery",
          identity_id: taken.identity.identity_id
        })

      assert rebind(conn, s.authed, reauth_token(s.authed), taken).status == 409
      assert rebind(conn, s.authed, "", bootstrap()).status == 401

      assert Lumia.Repo.get!(Accounts.Account, s.account.id).identity_id == s.old.identity_id
    end

    test "an account with no identity links instead", %{conn: conn} do
      {:ok, account} =
        Accounts.register(%{
          handle: "unbound#{System.unique_integer([:positive])}",
          password: "correct horse battery"
        })

      {:ok, _, tokens} = Lumia.Auth.create_session(account.id, Ecto.UUID.generate())
      authed = put_req_header(conn, "authorization", "Bearer " <> tokens.access_token)
      refused = rebind(conn, authed, reauth_token(authed), bootstrap())
      assert refused.status == 400

      assert %V1.Error{code: :FAILED_PRECONDITION, details: %{"state" => "unbound"}} =
               V1.Error.decode(refused.resp_body)
    end
  end

  test "linking takes only this session's reauth, never another account's", %{conn: conn} do
    register = fn ->
      {:ok, account} =
        Accounts.register(%{
          handle: "lk#{System.unique_integer([:positive])}",
          password: "correct horse battery"
        })

      {:ok, _, tokens} = Lumia.Auth.create_session(account.id, Ecto.UUID.generate())
      {account, put_req_header(conn, "authorization", "Bearer " <> tokens.access_token)}
    end

    {victim, victim_conn} = register.()
    {_attacker, attacker_conn} = register.()

    reauth =
      attacker_conn
      |> post_proto("/api/v1/auth/reauth", %V1.ReauthRequest{
        credential: {:password, "correct horse battery"}
      })
      |> Map.get(:resp_body)
      |> V1.ReauthResponse.decode()

    %{identity: identity, device: device, roster: roster} = bootstrap()

    refused =
      victim_conn
      |> put_req_header("x-reauth-token", reauth.reauth_token)
      |> post_proto("/api/v1/identity/link", proof(conn, identity, device, roster, :LINK_ACCOUNT))

    assert refused.status == 401
    assert Lumia.Repo.get!(Accounts.Account, victim.id).identity_id == nil
  end

  test "link bundles round-trip through the public fetch", %{conn: conn} do
    %{identity: identity, roster: v1} = bootstrap()

    {:ok, account} =
      Accounts.create_account(%{
        handle: "l#{System.unique_integer([:positive])}",
        identity_id: identity.identity_id
      })

    {:ok, _} = Identity.accept_roster(v1, :publish)
    {:ok, _, tokens} = Lumia.Auth.create_session(account.id, Ecto.UUID.generate())
    new_device = Ecto.UUID.generate()

    created =
      conn
      |> put_req_header("authorization", "Bearer " <> tokens.access_token)
      |> post_proto("/api/v1/identity/link-bundles", %V1.CreateLinkBundleRequest{
        ciphertext: "secret",
        sender_ephemeral_public_key: "e",
        nonce: "n",
        device_id: Proto.uuid(new_device)
      })

    assert created.status == 201

    %V1.CreateLinkBundleResponse{code: code} =
      V1.CreateLinkBundleResponse.decode(created.resp_body)

    fetched =
      conn
      |> put_req_header("accept", @proto)
      |> get("/api/v1/identity/link-bundles/#{code}?device_id=#{new_device}")

    assert %V1.GetLinkBundleResponse{ciphertext: "secret", host: host} =
             V1.GetLinkBundleResponse.decode(fetched.resp_body)

    assert host == Identity.host()

    assert get(conn, "/api/v1/identity/link-bundles/#{code}?device_id=#{new_device}").status ==
             404
  end

  test "a held roster is found by its current root, for recovery after a rotation" do
    %{identity: identity, device: phone, roster: v1} = bootstrap()
    {:ok, _} = Lumia.Identity.accept_roster(v1, :publish)
    new = identity()

    v2 =
      roster(identity, :root,
        version: 2,
        rotations: [rotation(identity, new.root_pub)],
        root_pub: new.root_pub,
        root_priv: new.root_priv,
        devices: [cert(identity, phone)]
      )

    {:ok, _} = Lumia.Identity.accept_roster(v2, :publish)

    by = fn root ->
      build_conn()
      |> put_req_header("accept", @proto)
      |> get("/api/v1/identity/rosters/by-root/#{Base.url_encode64(root, padding: false)}")
    end

    conn = by.(new.root_pub)
    assert conn.status == 200

    assert %V1.GetRosterResponse{roster: %V1.SignedRoster{body: body}} =
             V1.GetRosterResponse.decode(conn.resp_body)

    assert body == v2.body
    # The first root is history: only the current one finds it.
    assert by.(identity.root_pub).status == 404
    assert get(build_conn(), "/api/v1/identity/rosters/by-root/not-base64!").status == 400
  end
end
