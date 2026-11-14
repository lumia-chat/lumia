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
    @tag failure_injection: [identity: 8]
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

    test "a proof missing its roster, signature, or challenge expiry is invalid, not a crash" do
      %{identity: identity, device: device, roster: roster} = bootstrap()
      good = proof(identity, device, roster, :SIGN_IN)

      assert {:error, :identity_proof_invalid} =
               Identity.verify_proof(%V1.IdentityProof{}, :SIGN_IN)

      assert {:error, :identity_proof_invalid} =
               Identity.verify_proof(%{good | signature: nil}, :SIGN_IN)

      assert {:error, :identity_proof_invalid} =
               Identity.verify_proof(%{good | roster: nil}, :SIGN_IN)

      # A forged challenge with no expiry never reaches the MAC input.
      forged = V1.IdentityChallengeBody.encode(%V1.IdentityChallengeBody{nonce: "n", host: "h"})

      assert {:error, :identity_proof_invalid} =
               Identity.verify_proof(%{good | challenge: forged}, :SIGN_IN)
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

    @tag failure_injection: [identity: 13]
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
            cert(identity, web, :root, kind: :WEB, web_origin: Identity.own_origin())
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

      # Not newer than held and device-signed like it: ignored, and the
      # held version comes back.
      assert {:error, {:conflict, %{version: 2}}} =
               Identity.accept_roster(
                 roster(ctx.identity, ctx.phone,
                   version: 2,
                   devices: [cert(ctx.identity, ctx.phone)]
                 ),
                 :publish
               )
    end

    @tag failure_injection: [identity: 1, identity: 2]
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

    @tag failure_injection: [identity: 11]
    test "a root-signed revocation bypasses reauth (recovery path)", ctx do
      v2 =
        roster(ctx.identity, :root,
          version: 2,
          revoked: [revocation(ctx.identity, ctx.phone)],
          devices: [cert(ctx.identity, device("new"))]
        )

      assert {:ok, %{root_signed?: true}} = Identity.accept_roster(v2, :publish)
    end

    @tag failure_injection: [identity: 3]
    test "two device-signed forks at one version: the held one stays, the other gets CONFLICT with it, and the merged successor is accepted",
         ctx do
      laptop = device("laptop")
      tablet = device("tablet")
      phone_cert = cert(ctx.identity, ctx.phone)
      laptop_cert = cert(ctx.identity, laptop, ctx.phone, added_at: now(1))
      tablet_cert = cert(ctx.identity, tablet, ctx.phone, added_at: now(1))

      # Fork A reached this instance first.
      fork_a = roster(ctx.identity, ctx.phone, version: 2, devices: [phone_cert, laptop_cert])
      fork_b = roster(ctx.identity, ctx.phone, version: 2, devices: [phone_cert, tablet_cert])
      assert fork_a.body != fork_b.body
      assert {:ok, %{version: 2}} = Identity.accept_roster(fork_a, :publish)

      # Fork B is refused with the held bytes for the client to merge with.
      assert {:error, {:conflict, %{version: 2, signed: %{body: held_body}}}} =
               Identity.accept_roster(fork_b, :publish)

      assert held_body == fork_a.body
      assert Identity.held_roster(ctx.identity.identity_id).signed.body == fork_a.body

      # The merge (spec 5): the union at version 3, every entry with its own
      # signature, signed by the merging device.
      merged =
        roster(ctx.identity, ctx.phone,
          version: 3,
          devices: [phone_cert, laptop_cert, tablet_cert]
        )

      assert {:ok, %{version: 3, new_revocations: []} = verified} =
               Identity.accept_roster(merged, :publish)

      assert Enum.sort(Map.keys(verified.devices)) ==
               Enum.sort([ctx.phone.id, laptop.id, tablet.id])

      # An instance that held fork B instead accepts the same merged roster.
      assert {:ok, _} =
               Lumia.Identity.Roster.verify(merged,
                 held: elem(Lumia.Identity.Roster.verify(fork_b), 1)
               )
    end

    test "at the same version a root-signed roster beats a device-signed one, and never the reverse",
         ctx do
      laptop = device("laptop")
      fresh = device("fresh")

      thief =
        roster(ctx.identity, ctx.phone,
          version: 2,
          devices: [
            cert(ctx.identity, ctx.phone),
            cert(ctx.identity, laptop, ctx.phone, added_at: now(1))
          ]
        )

      assert {:ok, %{root_signed?: false}} = Identity.accept_roster(thief, :publish)

      # Recovery from the phrase: the root revokes the phone and adds a
      # fresh device, at the version the fork took.
      recovery =
        roster(ctx.identity, :root,
          version: 2,
          revoked: [revocation(ctx.identity, ctx.phone)],
          devices: [cert(ctx.identity, fresh)]
        )

      assert {:ok, %{root_signed?: true, version: 2, new_revocations: [phone_id]}} =
               Identity.accept_roster(recovery, :publish)

      assert phone_id == ctx.phone.id
      held = Identity.held_roster(ctx.identity.identity_id)
      assert held.signed.body == recovery.body
      assert held.root_signed?

      # Another device-signed roster at version 2 does not displace it.
      assert {:error, {:conflict, %{root_signed?: true}}} =
               Identity.accept_roster(thief, :publish)

      assert Identity.held_roster(ctx.identity.identity_id).signed.body == recovery.body
    end

    @tag failure_injection: [identity: 12]
    test "a root rotation is accepted by an instance holding the pre-rotation roster", ctx do
      new = identity()

      v2 =
        roster(ctx.identity, :root,
          version: 2,
          rotations: [rotation(ctx.identity, new.root_pub)],
          root_pub: new.root_pub,
          root_priv: new.root_priv,
          # The phone's certificate was signed by the old root and stays as it is.
          devices: [cert(ctx.identity, ctx.phone)]
        )

      assert {:ok, %{version: 2}} = Identity.accept_roster(v2, :publish)
      held = Identity.held_roster(ctx.identity.identity_id)
      assert held.version == 2

      # The old certificate still signs the device in under the rotated root.
      assert {:ok, verified, _} =
               Identity.verify_proof(proof(ctx.identity, ctx.phone, v2, :SIGN_IN), :SIGN_IN)

      assert verified.identity_id == ctx.identity.identity_id
      assert verified.root_public_key == new.root_pub
    end

    test "after a rotation the new root hands the backup key to the devices through their account logs",
         ctx do
      new = identity()

      v2 =
        roster(ctx.identity, :root,
          version: 2,
          rotations: [rotation(ctx.identity, new.root_pub)],
          root_pub: new.root_pub,
          root_priv: new.root_priv,
          devices: [cert(ctx.identity, ctx.phone)]
        )

      {:ok, _} = Identity.accept_roster(v2, :publish)

      handoff = fn version, root ->
        body =
          V1.BackupKeyHandoffBody.encode(%V1.BackupKeyHandoffBody{
            identity_id: ctx.identity.identity_id,
            roster_version: version,
            keys: [
              %V1.SealedBackupKey{
                device_id: ctx.phone.id,
                sender_ephemeral_public_key: :crypto.strong_rand_bytes(32),
                nonce: :crypto.strong_rand_bytes(24),
                ciphertext: :crypto.strong_rand_bytes(48)
              }
            ]
          })

        %V1.BackupKeyHandoff{
          body: body,
          signature: %V1.Signature{
            signer_kind: :ROOT,
            signer_id: root.root_pub,
            signature: Crypto.sign(body, root.root_priv)
          }
        }
      end

      # The old root cannot hand anything over once rotated away.
      assert {:error, {:identity_proof_invalid, :handoff, :signature}} =
               Identity.accept_backup_key_handoff(
                 handoff.(2, ctx.identity),
                 ctx.identity.identity_id
               )

      # A handoff for a version this instance does not hold waits for the roster.
      assert {:error, {:failed_precondition, "roster_version"}} =
               Identity.accept_backup_key_handoff(handoff.(3, new), ctx.identity.identity_id)

      # Another identity's device cannot put one on this identity's log.
      assert {:error, :permission_denied} =
               Identity.accept_backup_key_handoff(handoff.(2, new), identity().identity_id)

      accepted = handoff.(2, new)
      assert :ok = Identity.accept_backup_key_handoff(accepted, ctx.identity.identity_id)

      assert Enum.any?(Accounts.events_after(ctx.account.id, 0), fn event ->
               match?(
                 {:backup_key_handed_off, %V1.BackupKeyHandedOff{handoff: ^accepted}},
                 event.payload
               )
             end)
    end

    @tag failure_injection: [identity: 4]
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
    @tag failure_injection: [identity: 9, identity: 10]
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

      assert {:error, {:payload_too_large, 65_536}} = Identity.create_link_bundle(big)
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
