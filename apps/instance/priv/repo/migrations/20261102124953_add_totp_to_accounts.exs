defmodule Lumia.Repo.Migrations.AddTotpToAccounts do
  @moduledoc "TOTP as a second factor (architecture 6.3): a secret, enabled by its first valid code."

  use Ecto.Migration

  def change do
    alter table(:accounts) do
      add :totp_secret, :binary
      add :totp_enabled_at, :utc_datetime_usec
    end
  end
end
