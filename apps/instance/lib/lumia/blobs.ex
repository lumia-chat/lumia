defmodule Lumia.Blobs do
  @moduledoc """
  Blobs under envelope encryption (architecture 12).

  Each blob has a random 256-bit content key. The bytes are sealed with
  AES-256-GCM under it and that ciphertext is what the blob store keeps
  (`Lumia.Blobs.Store`), so the storage provider never sees plaintext. The
  content key is itself sealed under the operator's **instance master
  key** and kept in the blob's row. A blob's **address** is the keyed
  BLAKE3 of its stored ciphertext, under a key derived from the master key
  for the blob's kind: it is the blob's name in the store and in URLs, it
  cannot be computed by anyone without the master key, and since every
  upload has a fresh content key, the same bytes uploaded twice have two
  addresses (no deduplication here, and so no way to learn by uploading
  whether something is already held).

  The instance keeps blobs only when it has both a store and a master key
  (`enabled?/0`); without them every call answers `{:error, :unavailable}`
  and what would have used a blob says so to the person.

  What is built on this: account avatars (`Lumia.Accounts`), server
  icons and link cards' pictures (`Lumia.Cards`), all of them pictures the
  instance made itself. Attachments are not sealed here: their clients
  encrypt them and `Lumia.Uploads` keeps only rows about them.
  """

  import Ecto.Query

  alias Lumia.Blobs.Blob
  alias Lumia.Blobs.Store
  alias Lumia.Crypto
  alias Lumia.Repo

  # Each kind has a folder of its own in the store (`avatars/<hex>`), so an
  # operator can tell them apart, count them and treat them differently;
  # attachments are `att/` (`Lumia.Uploads`).
  @folders %{"avatar" => "avatars", "icon" => "icons", "card" => "cards"}

  @cipher :aes_256_gcm
  @content_aad "lumia blob v1"
  @key_aad "lumia blob key v1"

  @doc "Whether this instance keeps blobs: a store and a master key are configured."
  def enabled?, do: Store.configured() != nil and master_key() != nil

  @doc """
  Stores `bytes` as a blob of `kind` (`"avatar"` for an account,
  `"icon"` for a server, `"card"` for a link card's picture), of `media_type`, for
  `owner_account_id`. `{:ok, %Blob{}}`, or `{:error, :unavailable}` when
  the instance keeps no blobs or the store refused the write.
  """
  def put(bytes, kind, media_type, owner_account_id)
      when is_binary(bytes) and is_map_key(@folders, kind) do
    with master when is_binary(master) <- master_key() || {:error, :unavailable},
         true <- Store.configured() != nil || {:error, :unavailable} do
      content_key = :crypto.strong_rand_bytes(32)
      object = seal(content_key, bytes, @content_aad)
      address = Crypto.blake3_keyed(address_key(master, kind), object)

      blob = %Blob{
        address: address,
        kind: kind,
        media_type: media_type,
        size: byte_size(bytes),
        wrapped_key: seal(master, content_key, @key_aad <> address),
        owner_account_id: owner_account_id
      }

      # The object first: a row always has its object, and an object
      # whose row never landed is unreachable and only costs its bytes.
      with :ok <- Store.put(object_name(kind, address), object) |> as_unavailable() do
        Repo.insert(blob)
      end
    end
  end

  @doc """
  The blob at `address` if it is of `kinds` (one kind or a list):
  `{:ok, %{media_type: type, bytes: bytes}}`, or `{:error, :not_found}`
  for an address nothing is at, of another kind, or whose bytes do not
  open (a blob store that was tampered with, or another instance's master
  key).
  """
  def fetch(address, kinds) when is_binary(address) do
    kinds = List.wrap(kinds)

    with %Blob{kind: kind} = blob <- Repo.get(Blob, address),
         true <- kind in kinds,
         master when is_binary(master) <- master_key(),
         {:ok, object} <- Store.get(object_name(kind, address)),
         true <-
           Crypto.secure_compare(Crypto.blake3_keyed(address_key(master, kind), object), address),
         {:ok, content_key} <- open(master, blob.wrapped_key, @key_aad <> address),
         {:ok, bytes} <- open(content_key, object, @content_aad) do
      {:ok, %{media_type: blob.media_type, bytes: bytes}}
    else
      _ -> {:error, :not_found}
    end
  end

  @doc """
  Removes the blob at `address`, object and then row; fine when it is not
  there. `{:error, reason}` when the store would not let the object go,
  and the row stays to say it is there (`sweep_pictures/0` tries again).

  `only` (`kind: "icon"`, `owner_account_id: id`) names what the blob must
  be for it to go; another is left. An account's `avatar_url` can name a
  blob that is not its own (a signed profile may say anything), so what
  replaces a picture deletes only the picture it owned.
  """
  def delete(address, only \\ []) when is_binary(address) do
    case Repo.get(Blob, address) do
      nil -> :ok
      %Blob{} = blob -> if matches?(blob, only), do: delete_blob(blob), else: :ok
    end
  end

  defp matches?(blob, only),
    do: Enum.all?(only, fn {field, value} -> Map.fetch!(blob, field) == value end)

  defp delete_blob(%Blob{kind: kind, address: address}) do
    with :ok <- Store.delete(object_name(kind, address)) do
      Repo.delete_all(from b in Blob, where: b.address == ^address)
      :ok
    end
  end

  @doc """
  Removes account and server pictures more than an hour old that no
  account's `avatar_url` and no server's `icon_url` names: one whose
  replacement could not delete it (the store refused), or one left by a
  failure between keeping it and naming it. Hourly from `SweepMedia`.
  Returns how many went.
  """
  def sweep_pictures do
    before = DateTime.add(DateTime.utc_now(), -1, :hour)

    Repo.all(
      from b in Blob,
        as: :blob,
        where:
          b.kind in ["avatar", "icon"] and b.inserted_at < ^before and
            not exists(
              from a in "accounts",
                where:
                  a.avatar_url ==
                    fragment("'/api/v1/avatars/' || encode(?, 'hex')", parent_as(:blob).address),
                select: 1
            ) and
            not exists(
              from s in "servers",
                where:
                  s.icon_url ==
                    fragment("'/api/v1/avatars/' || encode(?, 'hex')", parent_as(:blob).address),
                select: 1
            ),
        limit: 1_000
    )
    |> Enum.count(&(delete_blob(&1) == :ok))
  end

  @doc "The names of the blobs the table has and the store does not, for a restore to report."
  def missing do
    for {kind, address} <- Repo.all(from b in Blob, select: {b.kind, b.address}),
        name = object_name(kind, address),
        not Store.exists?(name),
        do: name
  end

  @doc "A blob's name in a URL: its address in lowercase hex."
  def name(address), do: Base.encode16(address, case: :lower)

  @doc "Where a blob of `kind` at `address` is in the store: `avatars/<hex>` and the like."
  def object_name(kind, address), do: Map.fetch!(@folders, kind) <> "/" <> name(address)

  @doc """
  The address a blob's object would have as a blob of `kind`: the keyed
  BLAKE3 of the object under that kind's key. For moving a blob from one
  kind to another (`Lumia.Release.BlobFolders`).
  """
  def address_for(kind, object),
    do: Crypto.blake3_keyed(address_key(master_key(), kind), object)

  @doc """
  The content key of `blob` wrapped again for `new_address`, the master
  key's wrapping being bound to the address. `{:ok, wrapped}` or `:error`.
  """
  def rewrap(%Blob{} = blob, new_address) do
    master = master_key()

    with {:ok, content_key} <- open(master, blob.wrapped_key, @key_aad <> blob.address),
         do: {:ok, seal(master, content_key, @key_aad <> new_address)}
  end

  @doc "The address a name stands for, or nil when it is not one."
  def address(name) when is_binary(name) and byte_size(name) == 64 do
    case Base.decode16(name, case: :lower) do
      {:ok, address} -> address
      :error -> nil
    end
  end

  def address(_), do: nil

  @doc """
  The instance master key, 32 bytes, or nil when the operator gave none:
  the file `LUMIA_MASTER_KEY_FILE` names, holding the key as 32 raw bytes
  or as their base64 or hex. Read once.
  """
  def master_key do
    case :persistent_term.get({__MODULE__, :master_key}, :unread) do
      :unread ->
        key = read_master_key()
        :persistent_term.put({__MODULE__, :master_key}, key)
        key

      key ->
        key
    end
  end

  @doc false
  def forget_master_key, do: :persistent_term.erase({__MODULE__, :master_key})

  defp read_master_key do
    case {Application.get_env(:lumia, :master_key), Application.get_env(:lumia, :master_key_file)} do
      {<<key::binary-size(32)>>, _} -> key
      {_, path} when is_binary(path) -> path |> File.read!() |> parse_master_key!(path)
      _ -> nil
    end
  end

  defp parse_master_key!(<<key::binary-size(32)>>, _path), do: key

  defp parse_master_key!(contents, path) do
    text = String.trim(contents)

    decoded =
      case Base.decode64(text) do
        {:ok, key} -> {:ok, key}
        :error -> Base.decode16(text, case: :mixed)
      end

    case decoded do
      {:ok, <<key::binary-size(32)>>} -> key
      _ -> raise "#{path} is not a master key: 32 bytes, raw or as base64 or hex"
    end
  end

  defp address_key(master, kind),
    do: Crypto.blake3_keyed(master, "lumia blob address v1:" <> kind)

  # iv (12) || tag (16) || ciphertext.
  defp seal(key, plaintext, aad) do
    iv = :crypto.strong_rand_bytes(12)
    {ciphertext, tag} = :crypto.crypto_one_time_aead(@cipher, key, iv, plaintext, aad, true)
    iv <> tag <> ciphertext
  end

  defp open(key, <<iv::binary-size(12), tag::binary-size(16), ciphertext::binary>>, aad) do
    case :crypto.crypto_one_time_aead(@cipher, key, iv, ciphertext, aad, tag, false) do
      plaintext when is_binary(plaintext) -> {:ok, plaintext}
      :error -> :error
    end
  end

  defp open(_key, _sealed, _aad), do: :error

  defp as_unavailable(:ok), do: :ok

  defp as_unavailable({:error, reason}) do
    require Logger
    Logger.error("the blob store refused a write: #{inspect(reason)}")
    {:error, :unavailable}
  end
end
