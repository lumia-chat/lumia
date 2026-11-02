defmodule Lumia.Identity.RosterTest do
  use ExUnit.Case, async: true

  import Lumia.IdentityFixtures

  alias Lumia.Identity.Roster
  alias Lumia.V1

  describe "a valid roster" do
    test "v1 with one root-signed device verifies standalone" do
      %{identity: identity, device: device, roster: roster} = bootstrap()
      assert {:ok, verified} = Roster.verify(roster)
      assert verified.identity_id == identity.identity_id
      assert verified.version == 1
      assert Map.has_key?(verified.devices, device.id)
      assert verified.root_signed?
      assert verified.new_revocations == []
    end

    test "a device added by another device, roster signed by a device, verifies" do
      %{identity: identity, device: phone} = bootstrap()
      laptop = device("laptop")

      roster =
        roster(identity, phone,
          version: 2,
          devices: [cert(identity, phone), cert(identity, laptop, phone, added_at: now(1))]
        )

      assert {:ok, verified} = Roster.verify(roster)
      refute verified.root_signed?
      assert map_size(verified.devices) == 2
    end

    test "a rotation chain verifies and the identity id stays the first root" do
      %{identity: old, device: phone} = bootstrap()
      new = identity()
      rot = rotation(old, new.root_pub)

      roster =
        roster(old, :root,
          version: 2,
          rotations: [rot],
          root_pub: new.root_pub,
          root_priv: new.root_priv,
          devices: [cert(old, phone)]
        )

      assert {:ok, verified} = Roster.verify(roster)
      assert verified.identity_id == old.identity_id
      assert verified.root_public_key == new.root_pub
      assert verified.roots == [old.root_pub]
    end
  end

  describe "rejections, by step" do
    test "identity id must be the hash of the first root" do
      %{identity: identity, device: device} = bootstrap()
      wrong = %{identity | identity_id: :crypto.strong_rand_bytes(32)}
      roster = roster(wrong, :root, devices: [cert(wrong, device)])
      assert {:error, {:identity, :mismatch}} = Roster.verify(roster)
    end

    test "a certificate signed by an unknown device fails at certs" do
      %{identity: identity, device: phone} = bootstrap()
      stranger = device("stranger")

      roster =
        roster(identity, :root,
          devices: [cert(identity, phone), cert(identity, device("x"), stranger)]
        )

      assert {:error, {:certs, :signer}} = Roster.verify(roster)
    end

    test "a certificate signed before its signer was added fails at certs" do
      %{identity: identity, device: phone} = bootstrap()
      laptop = device("laptop")
      # Laptop's cert claims to be added before the phone that signed it.
      roster =
        roster(identity, :root,
          devices: [
            cert(identity, phone, :root, added_at: now(10)),
            cert(identity, laptop, phone, added_at: now(0))
          ]
        )

      assert {:error, {:certs, :signer}} = Roster.verify(roster)
    end

    test "a future-dated certificate fails at certs" do
      %{identity: identity, device: phone} = bootstrap()

      roster =
        roster(identity, :root, devices: [cert(identity, phone, :root, added_at: now(3600))])

      assert {:error, {:certs, :future_dated}} = Roster.verify(roster)
    end

    test "a tampered mirror field fails at parse" do
      %{identity: identity, device: phone} = bootstrap()
      entry = %{cert(identity, phone) | label: "ok", kind: :WEB}
      roster = roster(identity, :root, devices: [entry])
      assert {:error, {:parse, :mirror_mismatch}} = Roster.verify(roster)
    end

    test "a device revoking itself fails at revocations" do
      %{identity: identity, device: phone} = bootstrap()
      roster = roster(identity, :root, revoked: [revocation(identity, phone, phone)])
      assert {:error, {:revocations, :signer}} = Roster.verify(roster)
    end

    test "a roster signed by nobody authorized fails at signature" do
      %{identity: identity, device: phone} = bootstrap()
      stranger = device("stranger")
      roster = roster(identity, stranger, devices: [cert(identity, phone)])
      assert {:error, {:signature, :none_valid}} = Roster.verify(roster)
    end

    test "a broken rotation chain fails at chain" do
      %{identity: old, device: phone} = bootstrap()
      new = identity()
      other = identity()
      rot = rotation(old, other.root_pub)

      roster =
        roster(old, :root,
          version: 2,
          rotations: [rot],
          root_pub: new.root_pub,
          root_priv: new.root_priv,
          devices: [cert(old, phone)]
        )

      assert {:error, {:chain, :head_mismatch}} = Roster.verify(roster)
    end

    test "undecodable bytes fail at parse" do
      assert {:error, {:parse, :undecodable}} =
               Roster.verify(%V1.SignedRoster{body: "garbage", signatures: []})
    end
  end

  describe "monotonicity against a held version" do
    setup do
      %{identity: identity, device: phone, roster: v1} = bootstrap()
      {:ok, held} = Roster.verify(v1)
      %{identity: identity, phone: phone, held: held}
    end

    test "a newer superset is accepted and reports new revocations", ctx do
      laptop = device("laptop")

      v2 =
        roster(ctx.identity, ctx.phone,
          version: 2,
          devices: [
            cert(ctx.identity, ctx.phone),
            cert(ctx.identity, laptop, ctx.phone, added_at: now(1))
          ]
        )

      assert {:ok, held2} = Roster.verify(v2, held: ctx.held)
      assert held2.new_revocations == []

      v3 =
        roster(ctx.identity, ctx.phone,
          version: 3,
          devices: [cert(ctx.identity, ctx.phone)],
          revoked: [revocation(ctx.identity, laptop, ctx.phone)]
        )

      assert {:ok, held3} = Roster.verify(v3, held: held2)
      assert held3.new_revocations == [laptop.id]
    end

    test "the same or a lower version is rejected", ctx do
      v1_again = roster(ctx.identity, :root, version: 1, devices: [cert(ctx.identity, ctx.phone)])
      assert {:error, {:monotonic, :version}} = Roster.verify(v1_again, held: ctx.held)
    end

    test "dropping a device without revoking it is rejected", ctx do
      v2 = roster(ctx.identity, :root, version: 2, devices: [])
      assert {:error, {:monotonic, :device_dropped}} = Roster.verify(v2, held: ctx.held)
    end

    test "dropping a revocation is rejected", ctx do
      laptop = device("laptop")

      v2 =
        roster(ctx.identity, :root,
          version: 2,
          devices: [cert(ctx.identity, ctx.phone)],
          revoked: [revocation(ctx.identity, laptop)]
        )

      {:ok, held2} = Roster.verify(v2, held: ctx.held)
      v3 = roster(ctx.identity, :root, version: 3, devices: [cert(ctx.identity, ctx.phone)])
      assert {:error, {:monotonic, :revocation_dropped}} = Roster.verify(v3, held: held2)
    end
  end

  test "blake3 matches the reference vector" do
    assert Lumia.Crypto.blake3("") |> Base.encode16(case: :lower) ==
             "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
  end
end
