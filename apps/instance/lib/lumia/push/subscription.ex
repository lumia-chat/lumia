defmodule Lumia.Push.Subscription do
  @moduledoc "A device's push subscription of one kind (protocol `PushSubscription`)."

  use Lumia.Schema

  @kinds ~w(web_push unified_push relay_apns relay_fcm)

  schema "push_subscriptions" do
    field :account_id, Ecto.UUID
    field :device_id, Ecto.UUID
    field :kind, :string
    field :endpoint, :string
    field :p256dh, :string
    field :auth, :string
    timestamps()
  end

  def kinds, do: @kinds

  def changeset(subscription, attrs) do
    subscription
    |> Ecto.Changeset.cast(attrs, [:account_id, :device_id, :kind, :endpoint, :p256dh, :auth])
    |> Ecto.Changeset.validate_required([:account_id, :device_id, :kind, :endpoint])
    |> Ecto.Changeset.validate_inclusion(:kind, @kinds)
    |> Ecto.Changeset.validate_length(:endpoint, max: 2048)
    |> validate_web_push()
    |> Ecto.Changeset.unique_constraint([:device_id, :kind])
  end

  defp validate_web_push(changeset) do
    case Ecto.Changeset.get_field(changeset, :kind) do
      "web_push" ->
        changeset
        |> Ecto.Changeset.validate_required([:p256dh, :auth])
        |> Ecto.Changeset.validate_change(:endpoint, &endpoint_error/2)

      _ ->
        changeset
    end
  end

  defp endpoint_error(:endpoint, endpoint) do
    if push_service?(endpoint), do: [], else: [endpoint: "must be an https push service"]
  end

  # A push service is reached over https; a fake one on this machine, over
  # plain http on a loopback address, only where the configuration says so
  # (development and test, for the browser suite's push service).
  defp push_service?(endpoint) do
    case URI.parse(endpoint) do
      %URI{scheme: "https", host: host} when is_binary(host) and host != "" ->
        true

      %URI{scheme: "http", host: host} when host in ["localhost", "127.0.0.1", "::1"] ->
        Application.get_env(:lumia, :push_loopback_endpoints, false)

      _ ->
        false
    end
  end
end
