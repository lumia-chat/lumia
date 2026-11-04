defmodule Lumia.Push.Sender do
  @moduledoc """
  Where a push goes once the instance decided to send one: the push
  service for Web Push, the endpoint for UnifiedPush, the relay for APNs
  and FCM. The content is always the same generic notice; the app fetches
  the real thing itself (architecture 15.5). Configured as
  `:lumia, :push_sender`; the tests install one that reports to them.
  """

  alias Lumia.Push.Subscription

  @callback send(Subscription.t(), map()) :: :ok | {:error, term()}

  @doc "Sends through the configured sender."
  def send(%Subscription{} = subscription, notice) do
    impl = Application.get_env(:lumia, :push_sender, __MODULE__.Http)
    impl.send(subscription, notice)
  end

  defmodule Http do
    @moduledoc "The real one: Req to the push service, the endpoint, or the relay."
    @behaviour Lumia.Push.Sender

    alias Lumia.Push.WebPush

    @impl true
    def send(%Subscription{kind: "web_push"} = sub, notice) do
      settings = Lumia.Instance.settings()
      body = WebPush.encrypt(Jason.encode!(notice), sub.p256dh, sub.auth)

      headers =
        WebPush.headers(
          sub.endpoint,
          settings.vapid_public_key,
          settings.vapid_private_key,
          subject()
        )

      post(sub.endpoint, headers, body)
    end

    def send(%Subscription{kind: "unified_push"} = sub, notice) do
      # The app's push receiver learns which instance to sync from the
      # notice, as the relay's payload tells it for APNs and FCM.
      body = Jason.encode!(Map.put(notice, "instance", Lumia.Identity.host()))
      post(sub.endpoint, [{"content-type", "application/json"}], body)
    end

    def send(%Subscription{kind: "relay_" <> platform} = sub, notice) do
      case Application.get_env(:lumia, :push_relay_url) do
        nil ->
          {:error, :no_relay}

        relay ->
          payload = %{
            platform: platform,
            token: sub.endpoint,
            instance: Lumia.Identity.host(),
            notice: notice
          }

          post(relay <> "/push", [{"content-type", "application/json"}], Jason.encode!(payload))
      end
    end

    defp post(url, headers, body) do
      case Req.post(url, headers: headers, body: body, retry: false, receive_timeout: 10_000) do
        {:ok, %{status: status}} when status in 200..299 -> :ok
        {:ok, %{status: 410}} -> {:error, :gone}
        {:ok, %{status: 404}} -> {:error, :gone}
        {:ok, %{status: status}} -> {:error, {:status, status}}
        {:error, reason} -> {:error, reason}
      end
    end

    defp subject,
      do: "mailto:" <> Application.get_env(:lumia, :push_contact, "ops@" <> Lumia.Identity.host())
  end

  defmodule Test do
    @moduledoc "Reports every send to the process registered as `:lumia, :push_test_pid`."
    @behaviour Lumia.Push.Sender

    @impl true
    def send(subscription, notice) do
      case Application.get_env(:lumia, :push_test_pid) do
        pid when is_pid(pid) -> Kernel.send(pid, {:push, subscription, notice})
        _ -> :ok
      end

      :ok
    end
  end
end
