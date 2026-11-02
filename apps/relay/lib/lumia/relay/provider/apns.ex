defmodule Lumia.Relay.Provider.Apns do
  @moduledoc """
  Apple Push Notification service over HTTP/2 with token-based auth: a
  provider token (ES256 JWT from the `.p8` key, good for an hour) and a
  visible, mutable notice so the app's notification extension replaces it
  (architecture 15.5). Configured by `APNS_KEY_FILE`, `APNS_KEY_ID`,
  `APNS_TEAM_ID` and `APNS_TOPIC` (the app's bundle id).
  """

  @behaviour Lumia.Relay.Provider

  alias Lumia.Relay.Jwt

  @impl true
  def deliver(token, instance, notice) do
    config = Application.fetch_env!(:lumia_relay, :apns)
    {url, headers, body} = request(config, token, instance, notice)

    case Req.post(url,
           headers: headers,
           body: body,
           retry: false,
           receive_timeout: 10_000,
           connect_options: [protocols: [:http2]]
         ) do
      {:ok, %{status: 200}} -> :ok
      {:ok, %{status: 410}} -> {:error, :gone}
      {:ok, %{status: 400, body: %{"reason" => "BadDeviceToken"}}} -> {:error, :gone}
      {:ok, %{status: status, body: body}} -> {:error, {:status, status, body}}
      {:error, reason} -> {:error, reason}
    end
  end

  @doc "The request to send: URL, headers (with a fresh provider token) and the JSON body."
  def request(config, token, instance, notice) do
    host =
      if config[:sandbox],
        do: "https://api.sandbox.push.apple.com",
        else: "https://api.push.apple.com"

    now = System.os_time(:second)

    jwt =
      Jwt.es256(
        %{"kid" => config[:key_id]},
        %{"iss" => config[:team_id], "iat" => now},
        config[:private_key]
      )

    headers = [
      {"authorization", "bearer " <> jwt},
      {"apns-topic", config[:topic]},
      {"apns-push-type", "alert"},
      {"apns-priority", "10"},
      {"apns-expiration", Integer.to_string(now + 86_400)},
      {"content-type", "application/json"}
    ]

    body =
      Jason.encode!(%{
        "aps" => %{
          "alert" => %{
            "title" => notice["title"] || "Lumia",
            "body" => notice["body"] || "New message"
          },
          "mutable-content" => 1,
          "sound" => "default"
        },
        "instance" => instance
      })

    {host <> "/3/device/" <> token, headers, body}
  end
end
