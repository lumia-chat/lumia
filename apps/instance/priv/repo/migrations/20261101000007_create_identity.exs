defmodule Lumia.Repo.Migrations.CreateIdentity do
  @moduledoc """
  Identity storage (docs/specs/identity.md): the newest verified roster per
  identity, used challenges, link bundles, identity backups, reauth tokens.
  """

  use Ecto.Migration

  def change do
    create table(:identity_rosters, primary_key: false) do
      add :identity_id, :binary, null: false, primary_key: true
      add :version, :bigint, null: false
      add :root_public_key, :binary, null: false
      # Serialized Lumia.V1.SignedRoster, exactly as accepted.
      add :roster, :binary, null: false
      timestamps()
    end

    create table(:used_challenges, primary_key: false) do
      add :nonce, :binary, null: false, primary_key: true
      add :expires_at, :utc_datetime_usec, null: false
    end

    create index(:used_challenges, [:expires_at])

    create table(:link_bundles, primary_key: false) do
      add :code, :text, null: false, primary_key: true
      add :device_id, :binary_id, null: false
      add :ciphertext, :binary, null: false
      add :sender_ephemeral_public_key, :binary, null: false
      add :nonce, :binary, null: false
      add :attempts, :integer, null: false, default: 0
      add :expires_at, :utc_datetime_usec, null: false
      add :created_at, :utc_datetime_usec, null: false
    end

    create index(:link_bundles, [:expires_at])

    create table(:identity_backups, primary_key: false) do
      add :identity_id, :binary, null: false, primary_key: true
      add :version, :bigint, null: false
      add :ciphertext, :binary, null: false
      add :nonce, :binary, null: false
      timestamps()
    end

    create table(:reauth_tokens, primary_key: false) do
      add :token_hash, :binary, null: false, primary_key: true
      add :session_id, references(:sessions, on_delete: :delete_all), null: false
      add :expires_at, :utc_datetime_usec, null: false
      add :created_at, :utc_datetime_usec, null: false
    end

    create index(:reauth_tokens, [:session_id])
  end
end
