defmodule LumiaWeb.PushControllerTest do
  use LumiaWeb.ConnCase, async: true

  import Lumia.Fixtures

  alias Lumia.Auth
  alias Lumia.Push
  alias Lumia.V1

  @proto "application/x-protobuf"

  defp authed(conn, account, device_id) do
    {:ok, _session, tokens} = Auth.create_session(account.id, device_id)

    conn
    |> put_req_header("authorization", "Bearer " <> tokens.access_token)
    |> put_req_header("accept", @proto)
    |> put_req_header("content-type", @proto)
  end

  test "a device registers, replaces and removes its subscription; the trust card carries the VAPID key",
       %{conn: conn} do
    a = account("a")
    device = Ecto.UUID.generate()

    sub = %V1.PushSubscription{
      kind: :WEB_PUSH,
      endpoint: "https://push.example/abc",
      p256dh: Base.url_encode64(:crypto.strong_rand_bytes(65), padding: false),
      auth: Base.url_encode64(:crypto.strong_rand_bytes(16), padding: false)
    }

    assert (conn
            |> authed(a, device)
            |> put("/api/v1/push/subscription", V1.PushSubscription.encode(sub))).status == 204

    assert [%{device_id: ^device, kind: "web_push"}] = Push.for_accounts([a.id])

    bad = V1.PushSubscription.encode(%{sub | endpoint: "http://plain"})
    assert (conn |> authed(a, device) |> put("/api/v1/push/subscription", bad)).status == 400

    assert (conn |> authed(a, device) |> delete("/api/v1/push/subscription?kind=web_push")).status ==
             204

    assert Push.for_accounts([a.id]) == []

    assert (conn |> authed(a, device) |> delete("/api/v1/push/subscription?kind=pigeon")).status ==
             400

    info = conn |> put_req_header("accept", @proto) |> get("/api/v1/instance")
    assert %V1.InstanceInfo{web_push_public_key: key} = V1.InstanceInfo.decode(info.resp_body)
    assert byte_size(Base.url_decode64!(key, padding: false)) == 65
  end
end
