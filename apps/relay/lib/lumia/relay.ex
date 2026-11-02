defmodule Lumia.Relay do
  @moduledoc """
  The push relay (architecture 15.5): APNs and FCM accept pushes only
  from the app's own credentials, so self-hosted instances hand their
  content-free notices to this service, which holds the credentials and
  forwards them. It sees a device token, an instance name and a time,
  nothing more; the notice carries no content and the app fetches the
  message itself. Anyone can run one and point their own build at it.

  `deliver/4` is the whole job: pick the provider for the platform and
  send. Providers implement `Lumia.Relay.Provider`; `Fake` records what
  it is given, for the tests and for a relay run without credentials.
  """

  alias Lumia.Relay.Provider

  @doc "Forwards one notice; the platform is apns or fcm."
  def deliver(platform, token, instance, notice)
      when platform in ["apns", "fcm"] and is_binary(token) and is_binary(instance) do
    provider = Application.get_env(:lumia_relay, :providers, %{})[platform] || Provider.Fake
    provider.deliver(token, instance, notice)
  end

  def deliver(_platform, _token, _instance, _notice), do: {:error, :invalid_argument}
end
