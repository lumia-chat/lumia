defmodule Lumia.Crypto do
  @moduledoc """
  The primitives the identity spec names (docs/specs/identity.md section 2):
  Ed25519 from `:crypto`, BLAKE3 from `b3`, HMAC-SHA256 for challenges.

  This module is the only place that names the implementations, so a swap
  (a NIF for large blobs, for example) is one line.
  """

  @doc "BLAKE3-256 of `data`, 32 bytes."
  def blake3(data) when is_binary(data), do: B3.hash(data)

  @doc "Keyed BLAKE3-256 under a 32-byte key."
  def blake3_keyed(key, data) when byte_size(key) == 32 and is_binary(data),
    do: B3.keyed_hash(data, key)

  @doc "A fresh Ed25519 keypair as `{public, private}`."
  def generate_ed25519 do
    {public, private} = :crypto.generate_key(:eddsa, :ed25519)
    {public, private}
  end

  @doc "Ed25519 signature over `message`."
  def sign(message, private) when byte_size(private) == 32,
    do: :crypto.sign(:eddsa, :none, message, [private, :ed25519])

  @doc "Whether `signature` is a valid Ed25519 signature of `message` under `public`."
  def verify(message, signature, public)
      when byte_size(public) == 32 and byte_size(signature) == 64,
      do: :crypto.verify(:eddsa, :none, message, signature, [public, :ed25519])

  def verify(_message, _signature, _public), do: false

  @doc "HMAC-SHA256."
  def hmac(key, data), do: :crypto.mac(:hmac, :sha256, key, data)

  @doc "Constant-time binary comparison."
  def secure_compare(a, b) when is_binary(a) and is_binary(b), do: :crypto.hash_equals(a, b)
end
