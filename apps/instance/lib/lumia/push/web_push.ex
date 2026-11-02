defmodule Lumia.Push.WebPush do
  @moduledoc """
  Web Push as the instance serves it itself (architecture 15.5): message
  encryption per RFC 8291 (`aes128gcm`) and the VAPID authorization per
  RFC 8292, with nothing but `:crypto`. Safari accepts only `aes128gcm`,
  which is why the older `aesgcm` scheme is not offered.
  """

  @record_size 4096

  @doc """
  Encrypts `plaintext` for a subscription's `p256dh` and `auth` (base64url,
  unpadded), returning the `aes128gcm` body. `opts` may fix `:salt` and
  `:keypair` (`{public, private}`) for the RFC's test vector.
  """
  def encrypt(plaintext, p256dh, auth, opts \\ []) do
    ua_public = decode(p256dh)
    auth_secret = decode(auth)
    salt = Keyword.get_lazy(opts, :salt, fn -> :crypto.strong_rand_bytes(16) end)

    {as_public, as_private} =
      Keyword.get_lazy(opts, :keypair, fn -> :crypto.generate_key(:ecdh, :prime256v1) end)

    shared = :crypto.compute_key(:ecdh, ua_public, as_private, :prime256v1)
    prk_key = hmac(auth_secret, shared)
    ikm = expand(prk_key, "WebPush: info" <> <<0>> <> ua_public <> as_public, 32)
    prk = hmac(salt, ikm)
    cek = expand(prk, "Content-Encoding: aes128gcm" <> <<0>>, 16)
    nonce = expand(prk, "Content-Encoding: nonce" <> <<0>>, 12)

    {ciphertext, tag} =
      :crypto.crypto_one_time_aead(:aes_128_gcm, cek, nonce, plaintext <> <<2>>, "", true)

    salt <> <<@record_size::32, byte_size(as_public)::8>> <> as_public <> ciphertext <> tag
  end

  @doc """
  The `Authorization` header value for a push service `endpoint`: a
  12-hour ES256 JWT over the endpoint's origin, signed by the instance's
  VAPID key, with the public key beside it (`vapid t=…, k=…`).
  """
  def authorization(endpoint, public, private, subject) do
    %URI{scheme: scheme, host: host, port: port} = URI.parse(endpoint)
    audience = "#{scheme}://#{host}#{port_suffix(scheme, port)}"
    header = encode(Jason.encode!(%{typ: "JWT", alg: "ES256"}))
    exp = System.os_time(:second) + 12 * 3600
    claims = encode(Jason.encode!(%{aud: audience, exp: exp, sub: subject}))
    input = header <> "." <> claims
    der = :crypto.sign(:ecdsa, :sha256, input, [private, :prime256v1])
    "vapid t=#{input}.#{encode(der_to_raw(der))}, k=#{encode(public)}"
  end

  @doc "The headers of a push request carrying an `aes128gcm` body."
  def headers(endpoint, public, private, subject, ttl \\ 86_400) do
    [
      {"authorization", authorization(endpoint, public, private, subject)},
      {"content-encoding", "aes128gcm"},
      {"content-type", "application/octet-stream"},
      {"ttl", Integer.to_string(ttl)},
      {"urgency", "normal"}
    ]
  end

  # ---- RFC 5869 pieces and encodings ----------------------------------------------

  defp hmac(key, data), do: :crypto.mac(:hmac, :sha256, key, data)

  defp expand(prk, info, length) when length <= 32,
    do: binary_part(hmac(prk, info <> <<1>>), 0, length)

  # An ECDSA signature from :crypto is DER; JWS wants r || s, 32 bytes each.
  defp der_to_raw(<<0x30, _len, 0x02, rlen, rest::binary>>) do
    <<r::binary-size(^rlen), 0x02, slen, s::binary-size(slen)>> = rest
    pad(r) <> pad(s)
  end

  defp pad(int) do
    int = int |> :binary.decode_unsigned() |> :binary.encode_unsigned()
    :binary.copy(<<0>>, 32 - byte_size(int)) <> int
  end

  defp port_suffix("https", 443), do: ""
  defp port_suffix("http", 80), do: ""
  defp port_suffix(_, nil), do: ""
  defp port_suffix(_, port), do: ":#{port}"

  defp encode(bin), do: Base.url_encode64(bin, padding: false)
  defp decode(str), do: Base.url_decode64!(str, padding: false)
end
