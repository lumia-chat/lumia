defmodule Lumia.Auth.Totp do
  @moduledoc """
  Time-based one-time passwords as the second factor for password sign-in
  and for reauth (architecture 6.3, identity spec 10). The codes come from
  `NimbleTOTP` (RFC 6238: HMAC-SHA1, a 30-second step, 6 digits); this
  module adds the secret format, the enrolment URI, and one step of drift
  either way.
  """

  @period 30
  @drift 1

  @doc "A fresh 160-bit secret."
  def generate_secret, do: NimbleTOTP.secret()

  @doc "The secret as base32 without padding, for manual entry."
  def encode(secret), do: Base.encode32(secret, padding: false)

  @doc "The `otpauth://` URI for a QR code."
  def otpauth_uri(secret, label, issuer),
    do: NimbleTOTP.otpauth_uri("#{issuer}:#{label}", secret, issuer: issuer)

  @doc "The code for a point in time (default now)."
  def code(secret, %DateTime{} = at \\ DateTime.utc_now()),
    do: NimbleTOTP.verification_code(secret, time: at)

  @doc "Whether `code` is valid for the secret now, allowing one step of drift."
  def valid?(secret, code, at \\ DateTime.utc_now())

  def valid?(secret, code, %DateTime{} = at) when is_binary(secret) and is_binary(code) do
    code = String.trim(code)

    Enum.any?(-@drift..@drift, fn step ->
      NimbleTOTP.valid?(secret, code, time: DateTime.add(at, step * @period, :second))
    end)
  end

  def valid?(_secret, _code, _at), do: false
end
