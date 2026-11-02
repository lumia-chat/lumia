defmodule Lumia.History do
  @moduledoc """
  History backup storage (docs/specs/e2ee.md section 9): encrypted chunks
  and signed manifests per identity, held as opaque bytes under a quota.
  The instance verifies nothing about their content; clients check manifest
  signatures against rosters when they restore. Keyed by identity id, so an
  account with no identity cannot use it.
  """

  import Ecto.Query

  alias Lumia.History.Chunk
  alias Lumia.History.Manifest
  alias Lumia.Repo

  @default_quota 100 * 1024 * 1024

  @doc "The per-identity quota in bytes, from `:lumia, :history_quota_bytes`."
  def quota_bytes, do: Application.get_env(:lumia, :history_quota_bytes) || @default_quota

  @doc """
  Stores (or replaces) a chunk. `{:error, :resource_exhausted}` when the
  identity's usage would exceed the quota.
  """
  def put_chunk(identity_id, device_id, chunk_id, channel_id, ciphertext, nonce)
      when is_binary(ciphertext) and is_binary(nonce) do
    size = byte_size(ciphertext)

    Repo.transact(fn ->
      {used, quota} = usage(identity_id)
      existing = chunk_size(identity_id, chunk_id)

      if used - existing + size > quota do
        {:error, :resource_exhausted}
      else
        insert_chunk(identity_id, device_id, chunk_id, channel_id, ciphertext, nonce, size)
      end
    end)
    |> case do
      {:ok, :ok} -> :ok
      error -> error
    end
  end

  defp chunk_size(identity_id, chunk_id) do
    Repo.one(
      from c in Chunk,
        where: c.identity_id == ^identity_id and c.chunk_id == ^chunk_id,
        select: c.size
    ) || 0
  end

  defp insert_chunk(identity_id, device_id, chunk_id, channel_id, ciphertext, nonce, size) do
    now = DateTime.utc_now()

    Repo.insert(
      %Chunk{
        identity_id: identity_id,
        chunk_id: chunk_id,
        device_id: device_id,
        channel_id: channel_id,
        ciphertext: ciphertext,
        nonce: nonce,
        size: size,
        created_at: now
      },
      on_conflict: {:replace, [:device_id, :channel_id, :ciphertext, :nonce, :size, :created_at]},
      conflict_target: [:identity_id, :chunk_id]
    )
    |> case do
      {:ok, _} -> {:ok, :ok}
      {:error, _} -> {:error, :internal}
    end
  end

  @doc "A chunk: `{:ok, %{ciphertext, nonce}}` or `{:error, :not_found}`."
  def get_chunk(identity_id, chunk_id) do
    case Repo.get_by(Chunk, identity_id: identity_id, chunk_id: chunk_id) do
      nil -> {:error, :not_found}
      %Chunk{ciphertext: c, nonce: n} -> {:ok, %{ciphertext: c, nonce: n}}
    end
  end

  @doc "Deletes a chunk; absent is fine."
  def delete_chunk(identity_id, chunk_id) do
    Repo.delete_all(
      from c in Chunk, where: c.identity_id == ^identity_id and c.chunk_id == ^chunk_id
    )

    :ok
  end

  @doc """
  Stores a device's manifest unless the held version is equal or higher,
  in which case nothing changes (the spec's "a lower version is ignored").
  """
  def put_manifest(identity_id, device_id, version, bytes)
      when is_integer(version) and is_binary(bytes) do
    now = DateTime.utc_now()

    newer =
      from m in Manifest,
        where: m.version < ^version,
        update: [set: [version: ^version, manifest: ^bytes, updated_at: ^now]]

    Repo.insert_all(
      Manifest,
      [
        %{
          identity_id: identity_id,
          device_id: device_id,
          version: version,
          manifest: bytes,
          updated_at: now
        }
      ],
      on_conflict: newer,
      conflict_target: [:identity_id, :device_id]
    )

    :ok
  end

  @doc "Every device's manifest for the identity, as `[%{device_id, version, manifest}]`."
  def list_manifests(identity_id) do
    Repo.all(
      from m in Manifest,
        where: m.identity_id == ^identity_id,
        order_by: m.device_id,
        select: %{device_id: m.device_id, version: m.version, manifest: m.manifest}
    )
  end

  @doc "Deletes every chunk and manifest of the identity (backup turned off, account deleted)."
  def delete_all(identity_id) do
    Repo.transact(fn ->
      Repo.delete_all(from c in Chunk, where: c.identity_id == ^identity_id)
      Repo.delete_all(from m in Manifest, where: m.identity_id == ^identity_id)
      {:ok, :ok}
    end)
    |> elem(1)
  end

  @doc "`{used_bytes, quota_bytes}` for the identity."
  def usage(identity_id) do
    # sum over bigint is numeric in Postgres; the cast keeps it an integer.
    used =
      Repo.one(
        from c in Chunk,
          where: c.identity_id == ^identity_id,
          select: type(coalesce(sum(c.size), 0), :integer)
      )

    {used, quota_bytes()}
  end
end
