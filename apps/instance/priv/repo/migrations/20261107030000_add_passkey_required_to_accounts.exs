defmodule Lumia.Repo.Migrations.AddPasskeyRequiredToAccounts do
  @moduledoc "When set, a password sign-in must also present one of the account's passkeys."

  use Ecto.Migration

  def change do
    alter table(:accounts) do
      add :passkey_required_at, :utc_datetime_usec
    end
  end
end
