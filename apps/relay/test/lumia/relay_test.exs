defmodule Lumia.RelayTest do
  use ExUnit.Case, async: false

  import Plug.Conn
  import Plug.Test

  alias Lumia.Relay.Jwt
  alias Lumia.Relay.Provider.Apns
  alias Lumia.Relay.Provider.Fcm
  alias Lumia.Relay.Router

  setup do
    Process.register(self(), :lumia_relay_fake)
    on_exit(fn -> :ok end)
    :ok
  end

  defp push(body) do
    conn(:post, "/push", Jason.encode!(body))
    |> put_req_header("content-type", "application/json")
    |> Router.call(Router.init([]))
  end

  test "a push is forwarded to the platform's provider with the notice reduced to title and body" do
    conn =
      push(%{
        platform: "apns",
        token: "tok",
        instance: "chat.example",
        notice: %{title: "Lumia", body: "New message", extra: "no"}
      })

    assert conn.status == 204

    assert_receive {:delivered, "tok", "chat.example",
                    %{"title" => "Lumia", "body" => "New message"}}
  end

  test "a badge and a read pass through; anything else in a notice does not" do
    conn =
      push(%{
        platform: "apns",
        token: "tok",
        instance: "chat.example",
        notice: %{kind: "read", badge: 3, channel: "secret", title: 7}
      })

    assert conn.status == 204
    assert_receive {:delivered, "tok", "chat.example", notice}
    assert notice == %{"kind" => "read", "badge" => 3, "title" => 7}

    push(%{platform: "apns", token: "tok", instance: "x", notice: %{kind: "other", badge: -1}})
    assert_receive {:delivered, "tok", "x", %{}}
  end

  test "a read is a silent background push carrying the badge" do
    {_public, private} = :crypto.generate_key(:ecdh, :prime256v1)
    config = [private_key: private, key_id: "K", team_id: "T", topic: "chat.lumia", sandbox: true]

    {_url, headers, body} =
      Apns.request(config, "abc", "chat.example", %{"kind" => "read", "badge" => 2})

    assert {"apns-push-type", "background"} in headers
    assert {"apns-priority", "5"} in headers
    decoded = Jason.decode!(body)
    assert decoded["aps"] == %{"content-available" => 1, "badge" => 2}
    assert decoded["kind"] == "read"

    {_url, headers, body} =
      Apns.request(config, "abc", "chat.example", %{"body" => "New message", "badge" => 4})

    assert {"apns-push-type", "alert"} in headers
    assert %{"aps" => %{"badge" => 4, "alert" => %{}}} = Jason.decode!(body)
  end

  test "a dead token answers 410, a bad body 400, health 200" do
    assert push(%{platform: "fcm", token: "gone-1", instance: "x", notice: %{}}).status == 410
    assert push(%{platform: "sms", token: "t", instance: "x"}).status == 400
    assert push(%{token: "t"}).status == 400
    assert (conn(:get, "/health") |> Router.call(Router.init([]))).status == 200
  end

  test "the APNs request carries a provider token, the topic and a mutable visible alert" do
    {public, private} = :crypto.generate_key(:ecdh, :prime256v1)

    config = [
      private_key: private,
      key_id: "KEY1",
      team_id: "TEAM1",
      topic: "chat.lumia.ios",
      sandbox: true
    ]

    {url, headers, body} = Apns.request(config, "abc", "chat.example", %{"body" => "New message"})
    assert url == "https://api.sandbox.push.apple.com/3/device/abc"
    assert {"apns-topic", "chat.lumia.ios"} in headers
    {"authorization", "bearer " <> jwt} = List.keyfind(headers, "authorization", 0)
    [h, c, sig] = String.split(jwt, ".")

    assert %{"alg" => "ES256", "kid" => "KEY1"} =
             Jason.decode!(Base.url_decode64!(h, padding: false))

    assert %{"iss" => "TEAM1"} = Jason.decode!(Base.url_decode64!(c, padding: false))
    assert byte_size(Base.url_decode64!(sig, padding: false)) == 64

    assert %{
             "aps" => %{"mutable-content" => 1, "alert" => %{"body" => "New message"}},
             "instance" => "chat.example"
           } = Jason.decode!(body)

    _ = public
  end

  test "the FCM assertion is RS256 over the service account's claims and the message is data only" do
    rsa = :public_key.generate_key({:rsa, 2048, 65_537})

    config = [
      project_id: "lumia-1",
      client_email: "svc@lumia-1.iam.gserviceaccount.com",
      private_key: rsa
    ]

    [h, c, sig] = String.split(Fcm.assertion(config, 1_000), ".")
    assert %{"alg" => "RS256"} = Jason.decode!(Base.url_decode64!(h, padding: false))
    claims = Jason.decode!(Base.url_decode64!(c, padding: false))
    assert claims["iss"] == "svc@lumia-1.iam.gserviceaccount.com" and claims["exp"] == 4_600
    public = {:RSAPublicKey, elem(rsa, 2), elem(rsa, 3)}

    assert :public_key.verify(
             h <> "." <> c,
             :sha256,
             Base.url_decode64!(sig, padding: false),
             public
           )

    {url, _headers, body} = Fcm.request(config, "access", "tok", "chat.example", %{})
    assert url =~ "/projects/lumia-1/messages:send"

    assert %{"message" => %{"token" => "tok", "data" => %{"instance" => "chat.example"}}} =
             Jason.decode!(body)

    refute Map.has_key?(Jason.decode!(body)["message"], "notification")
  end

  test "an EC private key read from PEM yields its raw scalar" do
    {_, private} = :crypto.generate_key(:ecdh, :prime256v1)

    entry =
      :public_key.pem_entry_encode(
        :ECPrivateKey,
        {:ECPrivateKey, 1, private, {:namedCurve, {1, 2, 840, 10_045, 3, 1, 7}}, :asn1_NOVALUE,
         :asn1_NOVALUE}
      )

    assert :public_key.pem_encode([entry]) |> Jwt.private_key_from_pem() |> Jwt.ec_scalar() ==
             private
  end
end
