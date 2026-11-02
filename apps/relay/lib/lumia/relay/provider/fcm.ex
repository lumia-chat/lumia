defmodule Lumia.Relay.Provider.Fcm do
  @moduledoc """
  Firebase Cloud Messaging, HTTP v1: a service-account access token
  (RS256 JWT exchanged at Google's token endpoint, cached until it
  expires) and a data message the app turns into its own notification
  after fetching the content. Configured by `FCM_SERVICE_ACCOUNT_FILE`.
  """

  @behaviour Lumia.Relay.Provider

  alias Lumia.Relay.Jwt

  @scope "https://www.googleapis.com/auth/firebase.messaging"

  @impl true
  def deliver(token, instance, notice) do
    config = Application.fetch_env!(:lumia_relay, :fcm)

    with {:ok, access} <- access_token(config) do
      {url, headers, body} = request(config, access, token, instance, notice)

      case Req.post(url, headers: headers, body: body, retry: false, receive_timeout: 10_000) do
        {:ok, %{status: 200}} -> :ok
        {:ok, %{status: 404}} -> {:error, :gone}
        {:ok, %{status: status, body: body}} -> {:error, {:status, status, body}}
        {:error, reason} -> {:error, reason}
      end
    end
  end

  @doc "The request to send: URL, headers and the JSON body of a data message."
  def request(config, access_token, token, instance, notice) do
    headers = [
      {"authorization", "Bearer " <> access_token},
      {"content-type", "application/json"}
    ]

    body =
      Jason.encode!(%{
        "message" => %{
          "token" => token,
          "data" => %{
            "title" => notice["title"] || "Lumia",
            "body" => notice["body"] || "New message",
            "instance" => instance
          },
          "android" => %{"priority" => "high"}
        }
      })

    {"https://fcm.googleapis.com/v1/projects/#{config[:project_id]}/messages:send", headers, body}
  end

  @doc "The assertion that buys an access token: RS256 over the service account's claims."
  def assertion(config, now \\ System.os_time(:second)) do
    Jwt.rs256(
      %{},
      %{
        "iss" => config[:client_email],
        "scope" => @scope,
        "aud" => "https://oauth2.googleapis.com/token",
        "iat" => now,
        "exp" => now + 3600
      },
      config[:private_key]
    )
  end

  defp access_token(config) do
    now = System.os_time(:second)

    case :persistent_term.get({__MODULE__, :token}, nil) do
      {token, expires} when expires > now + 60 ->
        {:ok, token}

      _ ->
        form = [
          grant_type: "urn:ietf:params:oauth:grant-type:jwt-bearer",
          assertion: assertion(config)
        ]

        case Req.post("https://oauth2.googleapis.com/token", form: form, retry: false) do
          {:ok, %{status: 200, body: %{"access_token" => token, "expires_in" => ttl}}} ->
            :persistent_term.put({__MODULE__, :token}, {token, System.os_time(:second) + ttl})
            {:ok, token}

          {:ok, %{status: status, body: body}} ->
            {:error, {:oauth, status, body}}

          {:error, reason} ->
            {:error, reason}
        end
    end
  end
end
