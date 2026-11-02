defmodule Lumia.Repo.Migrations.CreateSessions do
  @moduledoc """
  Sessions (architecture 6.3, protocol section 12): one per device per
  instance, opaque tokens stored hashed, revocable.
  """

  use Ecto.Migration

  def change do
    create table(:sessions) do
      add :account_id, references(:accounts, on_delete: :delete_all), null: false
      add :device_id, :binary_id, null: false
      add :access_hash, :binary, null: false
      add :refresh_hash, :binary, null: false
      add :client, :text
      add :ip, :text
      add :access_expires_at, :utc_datetime_usec, null: false
      add :refresh_expires_at, :utc_datetime_usec, null: false
      add :last_seen_at, :utc_datetime_usec
      add :revoked_at, :utc_datetime_usec
      timestamps()
    end

    create unique_index(:sessions, [:access_hash])
    create unique_index(:sessions, [:refresh_hash])
    create index(:sessions, [:account_id])
    create index(:sessions, [:device_id])
  end
end
