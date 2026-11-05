defmodule Lumia.Repo.Migrations.CreateOidc do
  @moduledoc "OIDC sign-in (architecture 6.3): the provider identities bound to accounts, and the flows in progress."

  use Ecto.Migration

  def change do
    create table(:oidc_identities) do
      add :issuer, :text, null: false
      add :subject, :text, null: false
      add :account_id, references(:accounts, on_delete: :delete_all), null: false
      timestamps()
    end

    create unique_index(:oidc_identities, [:issuer, :subject])
    create index(:oidc_identities, [:account_id])

    create table(:oidc_flows) do
      add :state, :text, null: false
      add :session_params, :binary, null: false
      add :return_to, :text, null: false
      add :expires_at, :utc_datetime_usec, null: false
      timestamps(updated_at: false)
    end

    create unique_index(:oidc_flows, [:state])
  end
end
