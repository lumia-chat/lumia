defmodule Lumia.Relay.Jwt do
  @moduledoc "Just enough JWS for the two providers: ES256 (APNs) and RS256 (FCM's service account), on :crypto."

  @doc "Signs `claims` with `header` merged in, ES256 with a raw P-256 private key."
  def es256(header, claims, private_key) do
    input = signing_input(Map.put(header, "alg", "ES256"), claims)
    der = :crypto.sign(:ecdsa, :sha256, input, [private_key, :prime256v1])
    input <> "." <> encode(der_to_raw(der))
  end

  @doc "Signs `claims` RS256 with an RSA private key as `:public_key` decodes it."
  def rs256(header, claims, rsa_private_key) do
    input = signing_input(Map.put(header, "alg", "RS256"), claims)
    input <> "." <> encode(:public_key.sign(input, :sha256, rsa_private_key))
  end

  @doc "Reads a PEM file holding one private key (APNs `.p8` or a service account's key)."
  def private_key_from_pem(pem) do
    [entry] = :public_key.pem_decode(pem)
    :public_key.pem_entry_decode(entry)
  end

  @doc "The raw 32-byte scalar of a P-256 private key as :public_key decodes it."
  def ec_scalar({:ECPrivateKey, _, scalar, _, _, _}), do: scalar

  def ec_scalar({:PrivateKeyInfo, _, _, bytes, _}),
    do: ec_scalar(:public_key.der_decode(:ECPrivateKey, bytes))

  defp signing_input(header, claims) do
    encode(Jason.encode!(Map.put(header, "typ", "JWT"))) <> "." <> encode(Jason.encode!(claims))
  end

  defp der_to_raw(<<0x30, _len, 0x02, rlen, rest::binary>>) do
    <<r::binary-size(^rlen), 0x02, slen, s::binary-size(slen)>> = rest
    pad(r) <> pad(s)
  end

  defp pad(int) do
    int = int |> :binary.decode_unsigned() |> :binary.encode_unsigned()
    :binary.copy(<<0>>, 32 - byte_size(int)) <> int
  end

  defp encode(bin), do: Base.url_encode64(bin, padding: false)
end
