defmodule Lumia.Repo.Migrations.CreatePushSubscriptions do
  @moduledoc """
  Push subscriptions (architecture 15.5, protocol `PUT /push/subscription`):
  one per device and kind. The instance's VAPID key pair lives on the
  settings row, generated at first boot.
  """

  use Ecto.Migration

  def change do
    create table(:push_subscriptions) do
      add :account_id, references(:accounts, on_delete: :delete_all), null: false
      add :device_id, :uuid, null: false
      add :kind, :text, null: false
      add :endpoint, :text, null: false
      add :p256dh, :text
      add :auth, :text
      timestamps()
    end

    create unique_index(:push_subscriptions, [:device_id, :kind])
    create index(:push_subscriptions, [:account_id])

    alter table(:instance_settings) do
      add :vapid_public_key, :binary
      add :vapid_private_key, :binary
    end
  end
end
