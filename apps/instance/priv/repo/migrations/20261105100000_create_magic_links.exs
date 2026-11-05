defmodule Lumia.Repo.Migrations.CreateMagicLinks do
  @moduledoc "Magic-link sign-in tokens (architecture 6.3): hashed, short-lived, single use."

  use Ecto.Migration

  def change do
    create table(:magic_links) do
      add :account_id, references(:accounts, on_delete: :delete_all), null: false
      add :token_hash, :binary, null: false
      add :expires_at, :utc_datetime_usec, null: false
      add :used_at, :utc_datetime_usec
      timestamps(updated_at: false)
    end

    create unique_index(:magic_links, [:token_hash])
    create index(:magic_links, [:account_id, :inserted_at])
  end
end
