defmodule Lumia.Repo.Migrations.AddProfileToAccounts do
  @moduledoc "The account's signed profile, as the identity signed it (architecture 6.2, 6.3)."

  use Ecto.Migration

  def change do
    alter table(:accounts) do
      add :profile, :binary
    end
  end
end
