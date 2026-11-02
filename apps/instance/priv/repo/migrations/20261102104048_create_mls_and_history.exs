defmodule Lumia.Repo.Migrations.CreateMlsAndHistory do
  @moduledoc """
  The delivery-service duties from docs/specs/e2ee.md section 11: personal
  DM participants, the MLS epoch per conversation, key-package pools, the
  newest GroupInfo per conversation, and the history-backup store. The
  instance holds every MLS and history blob as opaque bytes.
  """

  use Ecto.Migration

  def change do
    alter table(:channels) do
      add :mls_epoch, :bigint, null: false, default: 0
      # sha256 hex of the sorted participant ids; the uniqueness constraint on
      # the pair (architecture 6.4).
      add :dm_key, :text
    end

    create unique_index(:channels, [:dm_key], where: "dm_key IS NOT NULL")

    create table(:dm_participants, primary_key: false) do
      add :channel_id, references(:channels, on_delete: :delete_all),
        null: false,
        primary_key: true

      add :account_id, references(:accounts, on_delete: :delete_all),
        null: false,
        primary_key: true
    end

    create index(:dm_participants, [:account_id])

    create table(:mls_key_packages) do
      add :account_id, references(:accounts, on_delete: :delete_all), null: false
      add :device_id, :binary_id, null: false
      add :key_package, :binary, null: false
      add :last_resort, :boolean, null: false, default: false
      add :expires_at, :utc_datetime_usec, null: false
      add :inserted_at, :utc_datetime_usec, null: false
    end

    create index(:mls_key_packages, [:account_id, :device_id, :last_resort])
    create index(:mls_key_packages, [:expires_at])

    create table(:mls_group_info, primary_key: false) do
      add :channel_id, references(:channels, on_delete: :delete_all),
        null: false,
        primary_key: true

      add :epoch, :bigint, null: false
      add :group_info, :binary, null: false
      add :updated_at, :utc_datetime_usec, null: false
    end

    create table(:history_chunks, primary_key: false) do
      add :identity_id, :binary, null: false, primary_key: true
      add :chunk_id, :binary_id, null: false, primary_key: true
      add :device_id, :binary_id, null: false
      add :channel_id, :binary_id, null: false
      add :ciphertext, :binary, null: false
      add :nonce, :binary, null: false
      add :size, :bigint, null: false
      add :created_at, :utc_datetime_usec, null: false
    end

    create index(:history_chunks, [:identity_id, :device_id])

    create table(:history_manifests, primary_key: false) do
      add :identity_id, :binary, null: false, primary_key: true
      add :device_id, :binary_id, null: false, primary_key: true
      add :version, :bigint, null: false
      # Serialized Lumia.V1.SignedHistoryManifest, exactly as uploaded.
      add :manifest, :binary, null: false
      add :updated_at, :utc_datetime_usec, null: false
    end
  end
end
