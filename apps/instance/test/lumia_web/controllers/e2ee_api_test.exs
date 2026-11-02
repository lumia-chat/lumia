defmodule LumiaWeb.E2eeApiTest do
  @moduledoc """
  The DM, MLS delivery-service and history endpoints over HTTP. Not async:
  the quota test changes application config, which `Lumia.HistoryTest`
  also does.
  """

  use LumiaWeb.ConnCase, async: false

  import Lumia.Fixtures

  alias Lumia.Accounts
  alias Lumia.Auth
  alias Lumia.Proto
  alias Lumia.V1

  @proto "application/x-protobuf"

  defp authed(conn, account, device_id \\ Ecto.UUID.generate()) do
    {:ok, _session, tokens} = Auth.create_session(account.id, device_id)

    conn
    |> put_req_header("authorization", "Bearer " <> tokens.access_token)
    |> put_req_header("accept", @proto)
  end

  defp send_proto(conn, method, path, struct) do
    conn = put_req_header(conn, "content-type", @proto)
    body = struct.__struct__.encode(struct)

    case method do
      :post -> post(conn, path, body)
      :put -> put(conn, path, body)
    end
  end

  defp decode(conn, module), do: module.decode(conn.resp_body)

  defp identity_account do
    {:ok, a} =
      Accounts.create_account(%{
        handle: "id#{System.unique_integer([:positive])}",
        identity_id: :crypto.strong_rand_bytes(32)
      })

    a
  end

  test "open a personal DM: 201 then 200 with existed", %{conn: conn} do
    a = account("a")
    b = account("b")
    req = %V1.OpenPersonalDmRequest{participant_account_ids: [Proto.uuid(b.id)]}

    first = conn |> authed(a) |> send_proto(:post, "/api/v1/dms/personal", req)
    assert first.status == 201

    assert %V1.OpenPersonalDmResponse{existed: false, channel: ch} =
             decode(first, V1.OpenPersonalDmResponse)

    assert ch.kind == :PERSONAL_DM

    again =
      conn
      |> authed(b)
      |> send_proto(:post, "/api/v1/dms/personal", %V1.OpenPersonalDmRequest{
        participant_account_ids: [Proto.uuid(a.id)]
      })

    assert again.status == 200

    assert %V1.OpenPersonalDmResponse{existed: true, channel: %{channel_id: id}} =
             decode(again, V1.OpenPersonalDmResponse)

    assert id == ch.channel_id
  end

  test "another account's roster is served only across a shared personal DM", %{conn: conn} do
    %{identity: identity, roster: roster} = Lumia.IdentityFixtures.bootstrap()
    {:ok, _} = Lumia.Identity.accept_roster(roster, :publish)

    {:ok, b} =
      Accounts.create_account(%{
        handle: "rb#{System.unique_integer([:positive])}",
        identity_id: identity.identity_id
      })

    a = account("a")
    stranger = account("s")

    assert (conn |> authed(stranger) |> get("/api/v1/accounts/#{b.id}/roster")).status == 404

    {:ok, ch, false} = Lumia.Dms.open_personal(a.id, [b.id])

    fetched = conn |> authed(a) |> get("/api/v1/accounts/#{b.id}/roster")
    assert fetched.status == 200
    assert %V1.GetRosterResponse{roster: ^roster} = decode(fetched, V1.GetRosterResponse)

    # The channel carries its participants and host for the client.
    assert Proto.channel(ch).participant_account_ids |> Enum.sort() ==
             Enum.sort([Proto.uuid(a.id), Proto.uuid(b.id)])

    assert Proto.channel(ch).host == Lumia.Identity.host()
  end

  test "key packages, claims, group info, and the epoch rule over HTTP", %{conn: conn} do
    %{channel: channel, a: a, b: b} = personal_dm()
    b_device = Ecto.UUID.generate()

    published =
      conn
      |> authed(b, b_device)
      |> send_proto(:post, "/api/v1/mls/key-packages", %V1.PublishKeyPackagesRequest{
        key_packages: ["kp1"],
        last_resort_key_package: "lr"
      })

    assert published.status == 204

    claimed =
      conn
      |> authed(a)
      |> send_proto(:post, "/api/v1/mls/key-packages/claim", %V1.ClaimKeyPackageRequest{
        account_id: Proto.uuid(b.id)
      })

    assert %V1.ClaimKeyPackageResponse{key_packages: [%{key_package: "kp1", device_id: dev}]} =
             decode(claimed, V1.ClaimKeyPackageResponse)

    assert Proto.from_uuid(dev) == b_device

    stranger = account("stranger")

    refused =
      conn
      |> authed(stranger)
      |> send_proto(:post, "/api/v1/mls/key-packages/claim", %V1.ClaimKeyPackageRequest{
        account_id: Proto.uuid(b.id)
      })

    assert refused.status == 404

    gi =
      conn
      |> authed(a)
      |> send_proto(:post, "/api/v1/mls/group-info", %V1.PublishGroupInfoRequest{
        channel_id: Proto.uuid(channel.id),
        group_info: "gi1",
        epoch: 1
      })

    assert gi.status == 204

    got = conn |> authed(b) |> get("/api/v1/mls/group-info/#{channel.id}")

    assert %V1.GetGroupInfoResponse{group_info: "gi1", epoch: 1} =
             decode(got, V1.GetGroupInfoResponse)

    assert (conn |> authed(stranger) |> get("/api/v1/mls/group-info/#{channel.id}")).status == 404

    wrong_epoch =
      conn
      |> authed(a)
      |> send_proto(
        :post,
        "/api/v1/channels/#{channel.id}/commands",
        send_encrypted(channel.id, :CONTROL, 5)
      )

    assert wrong_epoch.status == 400

    assert %V1.Error{code: :FAILED_PRECONDITION, details: %{"state" => "epoch"}} =
             decode(wrong_epoch, V1.Error)

    right =
      conn
      |> authed(a)
      |> send_proto(
        :post,
        "/api/v1/channels/#{channel.id}/commands",
        send_encrypted(channel.id, :CONTROL, 1)
      )

    assert %V1.Ack{seq: 1} = decode(right, V1.Ack)

    plain =
      conn
      |> authed(a)
      |> send_proto(:post, "/api/v1/channels/#{channel.id}/commands", send_message(channel.id))

    assert plain.status == 400
  end

  test "history: chunks, manifests, usage, quota, delete", %{conn: conn} do
    a = identity_account()
    device = Ecto.UUID.generate()
    chunk_id = Ecto.UUID.generate(version: 7)
    channel_id = Proto.uuid(Ecto.UUID.generate())

    put =
      conn
      |> authed(a, device)
      |> send_proto(:put, "/api/v1/history/chunks/#{chunk_id}", %V1.PutHistoryChunkRequest{
        channel_id: channel_id,
        ciphertext: "cipher",
        nonce: "nonce"
      })

    assert put.status == 204

    got = conn |> authed(a) |> get("/api/v1/history/chunks/#{chunk_id}")

    assert %V1.GetHistoryChunkResponse{ciphertext: "cipher", nonce: "nonce"} =
             decode(got, V1.GetHistoryChunkResponse)

    usage = conn |> authed(a) |> get("/api/v1/history/usage")

    assert %V1.HistoryUsageResponse{bytes_used: 6, bytes_quota: quota} =
             decode(usage, V1.HistoryUsageResponse)

    assert quota == Lumia.History.quota_bytes()

    body =
      V1.HistoryManifestBody.encode(%V1.HistoryManifestBody{
        identity_id: a.identity_id,
        version: 2
      })

    manifest = %V1.SignedHistoryManifest{
      body: body,
      signature: %V1.Signature{signer_kind: :DEVICE}
    }

    put_manifest =
      conn
      |> authed(a, device)
      |> send_proto(:put, "/api/v1/history/manifest", %V1.PutHistoryManifestRequest{
        manifest: manifest
      })

    assert put_manifest.status == 204

    list = conn |> authed(a) |> get("/api/v1/history/manifests")

    assert %V1.ListHistoryManifestsResponse{manifests: [^manifest]} =
             decode(list, V1.ListHistoryManifestsResponse)

    Application.put_env(:lumia, :history_quota_bytes, 8)
    on_exit(fn -> Application.delete_env(:lumia, :history_quota_bytes) end)

    over =
      conn
      |> authed(a, device)
      |> send_proto(
        :put,
        "/api/v1/history/chunks/#{Ecto.UUID.generate(version: 7)}",
        %V1.PutHistoryChunkRequest{
          channel_id: channel_id,
          ciphertext: "toolarge",
          nonce: "n"
        }
      )

    assert over.status == 507

    assert %V1.Error{code: :RESOURCE_EXHAUSTED, details: %{"quota" => "8"}} =
             decode(over, V1.Error)

    assert (conn |> authed(a) |> delete("/api/v1/history/chunks/#{chunk_id}")).status == 204
    assert (conn |> authed(a) |> get("/api/v1/history/chunks/#{chunk_id}")).status == 404
    assert (conn |> authed(a) |> delete("/api/v1/history")).status == 204

    assert %V1.ListHistoryManifestsResponse{manifests: []} =
             decode(
               conn |> authed(a) |> get("/api/v1/history/manifests"),
               V1.ListHistoryManifestsResponse
             )

    no_identity = conn |> authed(account("plain")) |> get("/api/v1/history/usage")
    assert no_identity.status == 400
    assert %V1.Error{code: :FAILED_PRECONDITION} = decode(no_identity, V1.Error)
  end
end
