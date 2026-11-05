defmodule Lumia.Repo.Migrations.CreatePasskeys do
  @moduledoc "Passkeys (WebAuthn credentials) and the ceremonies' pending challenges (architecture 6.3)."

  use Ecto.Migration

  def change do
    create table(:passkeys) do
      add :account_id, references(:accounts, on_delete: :delete_all), null: false
      add :credential_id, :binary, null: false
      # The COSE key as the library hands it over, term-encoded.
      add :public_key, :binary, null: false
      add :sign_count, :bigint, null: false, default: 0
      add :label, :text, null: false, default: ""
      add :last_used_at, :utc_datetime_usec
      timestamps()
    end

    create unique_index(:passkeys, [:credential_id])
    create index(:passkeys, [:account_id])

    create table(:webauthn_challenges) do
      add :kind, :text, null: false
      add :account_id, references(:accounts, on_delete: :delete_all)
      add :challenge, :binary, null: false
      add :expires_at, :utc_datetime_usec, null: false
      timestamps(updated_at: false)
    end

    create index(:webauthn_challenges, [:expires_at])
  end
end
