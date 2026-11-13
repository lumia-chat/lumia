defmodule Lumia.HistoryTest do
  # Not async: it changes the application's environment (a setting every
  # test reads), which a test running beside it would see.
  use Lumia.DataCase, async: false

  alias Lumia.History

  setup do
    %{identity: :crypto.strong_rand_bytes(32), device: Ecto.UUID.generate()}
  end

  defp chunk_id, do: Ecto.UUID.generate(version: 7)

  test "chunks round-trip, replace, and delete", ctx do
    id = chunk_id()
    channel = Ecto.UUID.generate()
    assert :ok = History.put_chunk(ctx.identity, ctx.device, id, channel, "ct", "n")
    assert {:ok, %{ciphertext: "ct", nonce: "n"}} = History.get_chunk(ctx.identity, id)
    assert {:error, :not_found} = History.get_chunk(:crypto.strong_rand_bytes(32), id)

    assert :ok = History.put_chunk(ctx.identity, ctx.device, id, channel, "ct-longer", "n2")
    assert {:ok, %{ciphertext: "ct-longer"}} = History.get_chunk(ctx.identity, id)
    assert {9, _} = History.usage(ctx.identity)

    assert :ok = History.delete_chunk(ctx.identity, id)
    assert :ok = History.delete_chunk(ctx.identity, id)
    assert {:error, :not_found} = History.get_chunk(ctx.identity, id)
    assert {0, _} = History.usage(ctx.identity)
  end

  test "the quota applies to the identity's total, counting a replacement once", ctx do
    Application.put_env(:lumia, :history_quota_bytes, 10)
    on_exit(fn -> Application.delete_env(:lumia, :history_quota_bytes) end)

    channel = Ecto.UUID.generate()
    first = chunk_id()
    assert :ok = History.put_chunk(ctx.identity, ctx.device, first, channel, "123456", "n")

    assert {:error, :resource_exhausted} =
             History.put_chunk(ctx.identity, ctx.device, chunk_id(), channel, "12345", "n")

    # Rewriting the first chunk at the same size fits; growing it does not.
    assert :ok = History.put_chunk(ctx.identity, ctx.device, first, channel, "abcdef", "n")

    assert {:error, :resource_exhausted} =
             History.put_chunk(ctx.identity, ctx.device, first, channel, "abcdefghijk", "n")

    assert :ok = History.put_chunk(ctx.identity, ctx.device, chunk_id(), channel, "1234", "n")
    assert {10, 10} = History.usage(ctx.identity)
  end

  test "manifests are per device and only move forward", ctx do
    other = Ecto.UUID.generate()
    assert :ok = History.put_manifest(ctx.identity, ctx.device, 2, "v2")
    assert :ok = History.put_manifest(ctx.identity, ctx.device, 1, "v1")
    assert :ok = History.put_manifest(ctx.identity, ctx.device, 2, "v2-again")
    assert :ok = History.put_manifest(ctx.identity, other, 1, "o1")

    assert [%{version: 1, manifest: "o1"}, %{version: 2, manifest: "v2"}] =
             ctx.identity
             |> History.list_manifests()
             |> Enum.sort_by(& &1.version)

    assert :ok = History.put_manifest(ctx.identity, ctx.device, 3, "v3")
    assert %{manifest: "v3"} = Enum.find(History.list_manifests(ctx.identity), &(&1.version == 3))
  end

  test "delete_all removes chunks and manifests of the identity only", ctx do
    other = :crypto.strong_rand_bytes(32)
    channel = Ecto.UUID.generate()
    :ok = History.put_chunk(ctx.identity, ctx.device, chunk_id(), channel, "a", "n")
    :ok = History.put_manifest(ctx.identity, ctx.device, 1, "m")
    :ok = History.put_chunk(other, ctx.device, chunk_id(), channel, "b", "n")
    :ok = History.put_manifest(other, ctx.device, 1, "m")

    assert :ok = History.delete_all(ctx.identity)
    assert {0, _} = History.usage(ctx.identity)
    assert History.list_manifests(ctx.identity) == []
    assert {1, _} = History.usage(other)
    assert [_] = History.list_manifests(other)
  end
end
