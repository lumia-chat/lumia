defmodule Lumia.MlsTest do
  use Lumia.DataCase, async: true

  import Lumia.Fixtures
  import Lumia.IdentityFixtures

  alias Lumia.Identity
  alias Lumia.Mls
  alias Lumia.Mls.KeyPackage
  alias Lumia.Proto

  setup do
    %{channel: channel, a: a, b: b} = personal_dm()
    %{channel: channel, a: a, b: b, device: Ecto.UUID.generate()}
  end

  describe "key packages" do
    test "publish counts, claim deletes, the last resort is served when empty and never deleted",
         ctx do
      assert {:ok, 2} = Mls.publish_key_packages(ctx.b.id, ctx.device, ["kp1", "kp2"], "lr1")
      assert Mls.key_package_count(ctx.b.id, ctx.device) == 2

      assert {:ok, [%{device_id: dev, key_package: "kp1"}]} = Mls.claim(ctx.a.id, ctx.b.id)
      assert dev == ctx.device
      assert {:ok, [%{key_package: "kp2"}]} = Mls.claim(ctx.a.id, ctx.b.id, ctx.device)
      assert Mls.key_package_count(ctx.b.id, ctx.device) == 0

      # Empty pool: the last resort, twice, still there afterwards.
      assert {:ok, [%{key_package: "lr1"}]} = Mls.claim(ctx.a.id, ctx.b.id)
      assert {:ok, [%{key_package: "lr1"}]} = Mls.claim(ctx.a.id, ctx.b.id)
      assert Repo.aggregate(KeyPackage, :count) == 1

      # The next top-up replaces it.
      assert {:ok, 1} = Mls.publish_key_packages(ctx.b.id, ctx.device, ["kp3"], "lr2")
      assert {:ok, [%{key_package: "kp3"}]} = Mls.claim(ctx.a.id, ctx.b.id)
      assert {:ok, [%{key_package: "lr2"}]} = Mls.claim(ctx.a.id, ctx.b.id)
      assert Repo.aggregate(KeyPackage, :count) == 1

      # An empty last-resort field leaves the previous one in place.
      assert {:ok, 1} = Mls.publish_key_packages(ctx.b.id, ctx.device, ["kp4"], "")
      assert Repo.aggregate(from(k in KeyPackage, where: k.last_resort), :count) == 1
    end

    test "one package per device of the target, or the named device only", ctx do
      other = Ecto.UUID.generate()
      {:ok, _} = Mls.publish_key_packages(ctx.b.id, ctx.device, ["d1"], "")
      {:ok, _} = Mls.publish_key_packages(ctx.b.id, other, ["d2"], "")

      assert {:ok, claimed} = Mls.claim(ctx.a.id, ctx.b.id)
      assert Enum.sort(Enum.map(claimed, & &1.device_id)) == Enum.sort([ctx.device, other])

      {:ok, _} = Mls.publish_key_packages(ctx.b.id, other, ["d3"], "")

      assert {:ok, [%{device_id: ^other, key_package: "d3"}]} =
               Mls.claim(ctx.a.id, ctx.b.id, other)

      # A device with nothing to serve is absent, not an error.
      assert {:ok, []} = Mls.claim(ctx.a.id, ctx.b.id)
    end

    test "claiming needs a shared personal channel", ctx do
      stranger = account("stranger")
      {:ok, _} = Mls.publish_key_packages(ctx.b.id, ctx.device, ["kp"], "lr")
      assert {:error, :not_found} = Mls.claim(stranger.id, ctx.b.id)
      assert Mls.key_package_count(ctx.b.id, ctx.device) == 1
    end

    test "purge_device and purge_expired", ctx do
      {:ok, _} = Mls.publish_key_packages(ctx.b.id, ctx.device, ["kp"], "lr")
      assert Mls.purge_device(ctx.b.id, ctx.device) == 2
      assert Repo.aggregate(KeyPackage, :count) == 0

      {:ok, _} = Mls.publish_key_packages(ctx.b.id, ctx.device, ["kp"], "")
      past = DateTime.add(DateTime.utc_now(), -1, :day)
      Repo.update_all(KeyPackage, set: [expires_at: past])
      assert Mls.key_package_count(ctx.b.id, ctx.device) == 0
      assert Mls.purge_expired() == 1
    end

    test "a revocation accepted by the roster purges the device's packages" do
      %{identity: identity, device: phone, roster: v1} = bootstrap()
      {:ok, _} = Identity.accept_roster(v1, :publish)

      {:ok, account} =
        Lumia.Accounts.create_account(%{
          handle: "id#{System.unique_integer([:positive])}",
          identity_id: identity.identity_id
        })

      laptop = device("laptop")
      laptop_id = Proto.from_uuid(laptop.id)

      v2 =
        roster(identity, phone,
          version: 2,
          devices: [cert(identity, phone), cert(identity, laptop, phone, added_at: now(1))]
        )

      {:ok, _} = Identity.accept_roster(v2, :publish)
      {:ok, _} = Mls.publish_key_packages(account.id, laptop_id, ["kp"], "lr")
      {:ok, _} = Mls.publish_key_packages(account.id, Proto.from_uuid(phone.id), ["kp"], "lr")

      v3 =
        roster(identity, :root,
          version: 3,
          devices: [cert(identity, phone)],
          revoked: [revocation(identity, laptop, phone)]
        )

      assert {:ok, %{new_revocations: [_]}} = Identity.accept_roster(v3, :publish)
      assert Repo.all(from k in KeyPackage, where: k.device_id == ^laptop_id) == []
      assert Mls.key_package_count(account.id, Proto.from_uuid(phone.id)) == 1
    end
  end

  describe "group info" do
    test "keeps the newest by epoch; participants only", ctx do
      stranger = account("stranger")
      assert {:error, :not_found} = Mls.get_group_info(ctx.channel.id, ctx.a.id)
      assert {:error, :not_found} = Mls.put_group_info(ctx.channel.id, stranger.id, 1, "gi")

      assert :ok = Mls.put_group_info(ctx.channel.id, ctx.a.id, 2, "gi2")
      assert :ok = Mls.put_group_info(ctx.channel.id, ctx.b.id, 1, "gi1")
      assert {:ok, %{epoch: 2, group_info: "gi2"}} = Mls.get_group_info(ctx.channel.id, ctx.b.id)

      assert :ok = Mls.put_group_info(ctx.channel.id, ctx.b.id, 2, "gi2-again")
      assert {:ok, %{group_info: "gi2"}} = Mls.get_group_info(ctx.channel.id, ctx.a.id)

      assert :ok = Mls.put_group_info(ctx.channel.id, ctx.b.id, 3, "gi3")
      assert {:ok, %{epoch: 3, group_info: "gi3"}} = Mls.get_group_info(ctx.channel.id, ctx.a.id)
      assert {:error, :not_found} = Mls.get_group_info(ctx.channel.id, stranger.id)
    end
  end
end
