defmodule Lumia.Repo.Migrations.ReauthByEveryCredential do
  @moduledoc """
  Reauth by every credential an account signs in with, and carried by a
  sign-in that proved one (specs/identity.md 10):

  - `sessions.credentialed_at`: when the session's sign-in proved a
    credential, so a handoff made soon after passes it on;
  - `magic_links.credentialed`: whether completing the token proves one
    (a mailed link and an OIDC login do, a handoff does not), and
    `magic_links.credentialed_at`, the one a handoff passes on from the
    session that made it;
  - `oidc_flows.session_id`: the session a reauth login is for;
  - `reauth_codes`: the six digits `POST /auth/reauth/magic-link` mails.
  """

  use Ecto.Migration

  def change do
    alter table(:sessions) do
      add :credentialed_at, :utc_datetime_usec
    end

    alter table(:magic_links) do
      add :credentialed, :boolean, null: false, default: true
      add :credentialed_at, :utc_datetime_usec
    end

    alter table(:oidc_flows) do
      add :session_id, references(:sessions, on_delete: :delete_all)
    end

    create table(:reauth_codes) do
      add :session_id, references(:sessions, on_delete: :delete_all), null: false
      add :account_id, references(:accounts, on_delete: :delete_all), null: false
      add :code_hash, :binary, null: false
      add :attempts, :integer, null: false, default: 0
      add :expires_at, :utc_datetime_usec, null: false
      timestamps(updated_at: false)
    end

    create index(:reauth_codes, [:session_id])
    create index(:reauth_codes, [:expires_at])
    create index(:reauth_codes, [:account_id, :inserted_at])
  end
end
