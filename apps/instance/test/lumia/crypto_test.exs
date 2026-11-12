defmodule Lumia.CryptoTest do
  use ExUnit.Case, async: true

  alias Lumia.Crypto

  test "BLAKE3 of nothing is its published vector" do
    assert Base.encode16(Crypto.blake3(""), case: :lower) ==
             "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
  end

  # The answer is the Rust `blake3` crate's, the implementation the client
  # core uses: `keyed_hash(&[0, 1, .., 31], b"lumia")`.
  test "keyed BLAKE3 agrees with the reference implementation" do
    key = :binary.list_to_bin(Enum.to_list(0..31))

    assert Base.encode16(Crypto.blake3_keyed(key, "lumia"), case: :lower) ==
             "c55583e88b15e0aefdb588abef024ba9ae09ff51da538749b85377fc32fdd4b6"
  end
end
