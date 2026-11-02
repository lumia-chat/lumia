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
        |> Ecto.Changeset.validate_format(:endpoint, ~r{^https://})

      _ ->
        changeset
    end
  end
end
