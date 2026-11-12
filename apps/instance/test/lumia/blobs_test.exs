defmodule Lumia.BlobsTest do
  use Lumia.DataCase, async: true

  import Lumia.Fixtures

  alias Lumia.Blobs
  alias Lumia.Blobs.Blob
  alias Lumia.Blobs.Store

  @picture String.duplicate("not really a picture, but long enough to find again ", 20)

  test "what is stored is ciphertext, and it opens to what was put" do
    owner = account()
    assert Blobs.enabled?()
    assert {:ok, %Blob{} = blob} = Blobs.put(@picture, "avatar", "image/png", owner.id)
    assert byte_size(blob.address) == 32
    assert blob.size == byte_size(@picture)

    {:ok, object} = Store.get(Blobs.name(blob.address))
    refute object =~ "not really a picture"
    refute blob.wrapped_key =~ "not really"
    # 12 of nonce and 16 of tag around the bytes; 32 of key the same way.
    assert byte_size(object) == byte_size(@picture) + 28
    assert byte_size(blob.wrapped_key) == 60

    assert {:ok, %{media_type: "image/png", bytes: @picture}} =
             Blobs.fetch(blob.address, "avatar")
  end

  test "the same bytes twice are two blobs at two addresses" do
    owner = account()
    {:ok, a} = Blobs.put(@picture, "avatar", "image/png", owner.id)
    {:ok, b} = Blobs.put(@picture, "avatar", "image/png", owner.id)
    assert a.address != b.address
  end

  test "an address nothing is at, another kind, and a store that was tampered with are not found" do
    owner = account()
    {:ok, blob} = Blobs.put(@picture, "avatar", "image/png", owner.id)

    assert {:error, :not_found} = Blobs.fetch(:crypto.strong_rand_bytes(32), "avatar")
    assert {:error, :not_found} = Blobs.fetch(blob.address, "attachment")

    name = Blobs.name(blob.address)
    {:ok, <<first, rest::binary>>} = Store.get(name)
    :ok = Store.put(name, <<Bitwise.bxor(first, 1), rest::binary>>)
    assert {:error, :not_found} = Blobs.fetch(blob.address, "avatar")

    # Another blob's object under this one's name does not pass for it either.
    {:ok, other} = Blobs.put("something else entirely", "avatar", "image/png", owner.id)
    {:ok, theirs} = Store.get(Blobs.name(other.address))
    :ok = Store.put(name, theirs)
    assert {:error, :not_found} = Blobs.fetch(blob.address, "avatar")
  end

  test "a deleted blob is gone from the table and from the store" do
    {:ok, blob} = Blobs.put(@picture, "avatar", "image/png", account().id)
    assert :ok = Blobs.delete(blob.address)
    assert {:error, :not_found} = Blobs.fetch(blob.address, "avatar")
    assert {:error, :not_found} = Store.get(Blobs.name(blob.address))
    assert :ok = Blobs.delete(blob.address)
  end

  test "a name is 64 of lowercase hex and nothing else" do
    address = :crypto.strong_rand_bytes(32)
    assert Blobs.address(Blobs.name(address)) == address
    assert Blobs.address(String.upcase(Blobs.name(address))) == nil
    assert Blobs.address("../../etc/passwd") == nil
    assert Blobs.address("") == nil
  end
end
