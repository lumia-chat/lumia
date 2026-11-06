defmodule Lumia.ProtoJSONTest do
  use ExUnit.Case, async: true

  alias Lumia.Proto
  alias Lumia.ProtoJSON
  alias Lumia.V1

  @account "01a1b2c3-d4e5-7f60-8a9b-0c1d2e3f4a5b"
  @server "01a1b2c3-d4e5-7f60-8a9b-0c1d2e3f4a5c"

  test "ids are UUID strings, hashes and nonces hex, the rest ProtoJSON" do
    identity = :crypto.hash(:sha256, "root")

    account = %V1.Account{
      account_id: Proto.uuid(@account),
      identity_id: identity,
      kind: :ACCOUNT_HUMAN,
      handle: "ryan",
      created_at: Proto.timestamp(~U[2026-11-06 12:00:00Z]),
      profile: %V1.SignedProfile{body: "body"}
    }

    map = Jason.decode!(ProtoJSON.encode!(account))
    assert map["accountId"] == @account
    assert map["identityId"] == Base.encode16(identity, case: :lower)
    assert map["kind"] == "ACCOUNT_HUMAN"
    assert map["createdAt"] == "2026-11-06T12:00:00Z"

    assert map["profile"] == %{
             "body" => Base.encode64("body"),
             "signature" => Base.encode64("sig")
           }

    assert ProtoJSON.decode!(ProtoJSON.encode!(account), V1.Account) == account

    nonce = :crypto.strong_rand_bytes(24)
    backup = %V1.PutIdentityBackupRequest{version: 12_345_678_901, ciphertext: "c", nonce: nonce}
    map = Jason.decode!(ProtoJSON.encode!(backup))
    assert map["version"] == "12345678901"
    assert map["nonce"] == Base.encode16(nonce, case: :lower)
    assert map["ciphertext"] == Base.encode64("c")
    assert ProtoJSON.decode!(ProtoJSON.encode!(backup), V1.PutIdentityBackupRequest) == backup
  end

  test "repeated ids, and a stock ProtoJSON client's base64 ids, decode" do
    ids = [Proto.uuid(@account), Proto.uuid(@server)]
    update = %V1.PresenceUpdate{status: :ONLINE, watch_server_ids: ids}
    map = Jason.decode!(ProtoJSON.encode!(update))
    assert map["watchServerIds"] == [@account, @server]
    assert ProtoJSON.decode!(ProtoJSON.encode!(update), V1.PresenceUpdate) == update
    assert ProtoJSON.decode!(Protobuf.JSON.encode!(update), V1.PresenceUpdate) == update
  end

  test "an id that is not sixteen bytes reads back from hex, inside a frame" do
    frame = %V1.ClientFrame{frame: {:hello, %V1.Hello{device_id: "not-a-uuid-at-all"}}}
    map = Jason.decode!(ProtoJSON.encode!(frame))
    assert map["hello"]["deviceId"] == Base.encode16("not-a-uuid-at-all", case: :lower)
    assert ProtoJSON.decode!(ProtoJSON.encode!(frame), V1.ClientFrame) == frame
  end
end
