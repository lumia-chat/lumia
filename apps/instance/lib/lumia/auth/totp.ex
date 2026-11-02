defmodule Lumia.Auth.Totp do
  @moduledoc """
  Time-based one-time passwords (RFC 6238 over RFC 4226): HMAC-SHA1, a
  30-second step, 6 digits, one step of drift either way. The second factor
  for password sign-in and for reauth (architecture 6.3, identity spec 10).
  """

  import Bitwise

  @step 30
  @digits 6
  @drift 1

  @doc "A fresh 160-bit secret."
  def generate_secret, do: :crypto.strong_rand_bytes(20)

  @doc "The secret as base32 without padding, for manual entry."
  def encode(secret), do: Base.encode32(secret, padding: false)

  @doc "The `otpauth://` URI for a QR code."
  def otpauth_uri(secret, label, issuer) do
    query =
      URI.encode_query(
        secret: encode(secret),
        issuer: issuer,
        algorithm: "SHA1",
        digits: @digits,
        period: @step
      )

    "otpauth://totp/" <> URI.encode("#{issuer}:#{label}") <> "?" <> query
  end

  @doc "The code for a counter step."
  def code_at(secret, counter) do
    hmac = :crypto.mac(:hmac, :sha, secret, <<counter::64>>)
    offset = :binary.last(hmac) &&& 0x0F
    <<_::binary-size(^offset), value::32, _::binary>> = hmac

    (value &&& 0x7FFFFFFF)
    |> rem(Integer.pow(10, @digits))
    |> Integer.to_string()
    |> String.pad_leading(@digits, "0")
  end

  @doc "The code for a point in time (default now)."
  def code(secret, %DateTime{} = at \\ DateTime.utc_now()),
    do: code_at(secret, div(DateTime.to_unix(at), @step))

  @doc "Whether `code` is valid for the secret now, allowing one step of drift."
  def valid?(secret, code, at \\ DateTime.utc_now())

  def valid?(secret, code, %DateTime{} = at) when is_binary(secret) and is_binary(code) do
    counter = div(DateTime.to_unix(at), @step)

    Enum.any?(-@drift..@drift, fn d ->
      Plug.Crypto.secure_compare(code_at(secret, counter + d), String.trim(code))
    end)
  end

  def valid?(_secret, _code, _at), do: false
end
