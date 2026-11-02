defmodule Lumia.IdentityTest do
  use Lumia.DataCase, async: true

  import Lumia.Fixtures
  import Lumia.IdentityFixtures

  alias Lumia.Accounts
  alias Lumia.Auth
  alias Lumia.Crypto
  alias Lumia.Identity
  alias Lumia.Proto
  alias Lumia.V1

  defp proof(identity, device, roster, purpose) do
    {challenge, _} = Identity.issue_challenge(purpose)

    %V1.IdentityProof{
      challenge: challenge,
      identity_id: identity.identity_id,
      device_id: device.id,
      signature: %V1.Signature{
        signer_kind: :DEVICE,
        signer_id: device.id,
        signature: Crypto.sign(challenge, device.priv)
      },
      roster: roster
    }
  end

  defp bound_account(identity) do
    {:ok, account} =
      Accounts.create_account(%{
        handle: "id#{System.unique_integer([:positive])}",
        identity_id: identity.identity_id
      })

    account
  end

  describe "challenges and proofs" do
    test "a valid proof verifies, stores the roster, and consumes the nonce" do
      %{identity: identity, device: device, roster: roster} = bootstrap()
      proof = proof(identity, device, roster, :SIGN_IN)

      assert {:ok, verified, found} = Identity.verify_proof(proof, :SIGN_IN)
      assert verified.identity_id == identity.identity_id
      assert found.cert.device_id == device.id
      assert Identity.held_roster(identity.identity_id).version == 1

      # Replay: same challenge bytes again.
      assert {:error, :identity_proof_invalid} = Identity.verify_proof(proof, :SIGN_IN)
    end

    test "wrong purpose, tampered mac, and unauthorized device are rejected" do
      %{identity: identity, device: device, roster: roster} = bootstrap()

      assert {:error, :identity_proof_invalid} =
               Identity.verify_proof(proof(identity, device, roster, :SIGN_IN), :CREATE_ACCOUNT)

      p = proof(identity, device, roster, :SIGN_IN)
      tampered = %{p | challenge: p.challenge <> <<0>>}
      assert {:error, :identity_proof_invalid} = Identity.verify_proof(tampered, :SIGN_IN)

      stranger = device("stranger")

      bad = %{
        p
        | device_id: stranger.id,
          signature: %V1.Signature{
            signer_kind: :DEVICE,
            signer_id: stranger.id,
            signature: Crypto.sign(p.challenge, stranger.priv)
          }
      }

      assert {:error, :identity_proof_invalid} = Identity.verify_proof(bad, :SIGN_IN)
    end

    test "a web device from another origin is rejected" do
      identity = identity()
      web = device("browser")

      roster =
        roster(identity, :root,
          devices: [cert(identity, web, :root, kind: :WEB, web_origin: "https://evil.example")]
        )

      assert {:error, :identity_proof_invalid} =
               Identity.verify_proof(proof(identity, web, roster, :SIGN_IN), :SIGN_IN)

      ok_roster =
        roster(identity, :root,
          version: 2,
          devices: [
            cert(identity, web, :root, kind: :WEB, web_origin: "https://" <> Identity.host())
          ]
        )

      assert {:ok, _, _} =
               Identity.verify_proof(proof(identity, web, ok_roster, :SIGN_IN), :SIGN_IN)
    end
  end

  describe "roster acceptance and the revocation policy" do
    setup do
      %{identity: identity, device: phone, roster: v1} = bootstrap()
      {:ok, _} = Identity.accept_roster(v1, :publish)
      account = bound_account(identity)
      %{identity: identity, phone: phone, account: account}
    end

    test "publishing an add-only roster is accepted; the same bytes again are a no-op", ctx do
      laptop = device("laptop")

      v2 =
        roster(ctx.identity, ctx.phone,
          version: 2,
          devices: [
            cert(ctx.identity, ctx.phone),
            cert(ctx.identity, laptop, ctx.phone, added_at: now(1))
          ]
        )

      assert {:ok, %{version: 2}} = Identity.accept_roster(v2, :publish)
      assert {:ok, %{version: 2, new_revocations: []}} = Identity.accept_roster(v2, :publish)

      assert {:error, :conflict} =
               Identity.accept_roster(
                 roster(ctx.identity, :root,
                   version: 2,
                   devices: [cert(ctx.identity, ctx.phone)]
                 ),
                 :publish
               )
    end

    test "a device-signed revocation is refused on publish and accepted with reauth", ctx do
      laptop = device("laptop")

      v2 =
        roster(ctx.identity, ctx.phone,
          version: 2,
          devices: [
            cert(ctx.identity, ctx.phone),
            cert(ctx.identity, laptop, ctx.phone, added_at: now(1))
          ]
        )

      {:ok, _} = Identity.accept_roster(v2, :publish)

      {:ok, _laptop_session, laptop_tokens} =
        Auth.create_session(ctx.account.id, Proto.from_uuid(laptop.id))

      v3 =
        roster(ctx.identity, ctx.phone,
          version: 3,
          devices: [cert(ctx.identity, ctx.phone)],
          revoked: [revocation(ctx.identity, laptop, ctx.phone)]
        )

      assert {:error, :policy_denied} = Identity.accept_roster(v3, :publish)

      assert {:error, :policy_denied} =
               Identity.accept_roster(v3, {:revoke, "bogus", ctx.account.id})

      {:ok, phone_session, _} = Auth.create_session(ctx.account.id, Proto.from_uuid(ctx.phone.id))
      {reauth, _} = Identity.issue_reauth(phone_session)

      assert {:ok, %{new_revocations: [_]}} =
               Identity.accept_roster(v3, {:revoke, reauth, ctx.account.id})

      # The laptop's session is gone, the account log says so, and the token was single use.
      assert {:error, :session_revoked} = Auth.verify_access(laptop_tokens.access_token)
      assert "roster_updated" in Accounts.event_kinds(ctx.account.id)
      assert {:error, :unauthenticated} = Identity.verify_reauth(reauth)
    end

    test "a root-signed revocation bypasses reauth (recovery path)", ctx do
      v2 =
        roster(ctx.identity, :root,
          version: 2,
          revoked: [revocation(ctx.identity, ctx.phone)],
          devices: [cert(ctx.identity, device("new"))]
        )

      assert {:ok, %{root_signed?: true}} = Identity.accept_roster(v2, :publish)
    end

    test "a roster that drops a revocation names the step", ctx do
      laptop = device("laptop")

      v2 =
        roster(ctx.identity, :root,
          version: 2,
          devices: [cert(ctx.identity, ctx.phone)],
          revoked: [revocation(ctx.identity, laptop)]
        )

      {:ok, _} = Identity.accept_roster(v2, :publish)
      v3 = roster(ctx.identity, :root, version: 3, devices: [cert(ctx.identity, ctx.phone)])

      assert {:error, {:identity_proof_invalid, :monotonic, :revocation_dropped}} =
               Identity.accept_roster(v3, :publish)
    end
  end

  describe "link bundles" do
    test "single use, wrong device counts attempts, fifth attempt deletes" do
      device_id = Ecto.UUID.generate()

      req = %V1.CreateLinkBundleRequest{
        ciphertext: "c",
        sender_ephemeral_public_key: "e",
        nonce: "n",
        device_id: Proto.uuid(device_id)
      }

      {:ok, bundle} = Identity.create_link_bundle(req)
      assert String.length(bundle.code) == 8

      for _ <- 1..4,
          do:
            assert(
              {:error, :not_found} = Identity.claim_link_bundle(bundle.code, Ecto.UUID.generate())
            )

      assert {:ok, %{ciphertext: "c"}} =
               Identity.claim_link_bundle(String.downcase(bundle.code), device_id)

      assert {:error, :not_found} = Identity.claim_link_bundle(bundle.code, device_id)

      {:ok, b2} = Identity.create_link_bundle(req)
      for _ <- 1..5, do: Identity.claim_link_bundle(b2.code, Ecto.UUID.generate())
      assert {:error, :not_found} = Identity.claim_link_bundle(b2.code, device_id)
    end

    test "oversized bundles are refused" do
      big = %V1.CreateLinkBundleRequest{
        ciphertext: :binary.copy(<<0>>, 70_000),
        device_id: Proto.uuid(Ecto.UUID.generate())
      }

      assert {:error, :payload_too_large} = Identity.create_link_bundle(big)
    end
  end

  describe "backups" do
    test "the highest version wins" do
      id = :crypto.strong_rand_bytes(32)
      assert :ok = Identity.put_backup(id, 2, "two", "n2")
      assert :ok = Identity.put_backup(id, 1, "one", "n1")
      assert %{version: 2, ciphertext: "two"} = Identity.get_backup(id)
      assert :ok = Identity.put_backup(id, 3, "three", "n3")
      assert %{version: 3} = Identity.get_backup(id)
    end
  end

  describe "linking" do
    test "binds an unbound account once" do
      account = account()
      id = :crypto.strong_rand_bytes(32)
      assert {:ok, _} = Identity.link_account(account.id, id)
      assert {:ok, _} = Identity.link_account(account.id, id)

      assert {:error, :conflict} =
               Identity.link_account(account.id, :crypto.strong_rand_bytes(32))
    end
  end
end
