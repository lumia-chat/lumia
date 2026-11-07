defmodule Lumia.Repo.Migrations.AddPronounsAndBioToAccounts do
  @moduledoc "Pronouns and bio taken from the verified profile, as the display name is."

  use Ecto.Migration

  def change do
    alter table(:accounts) do
      add :pronouns, :string
      add :bio, :text
    end
  end
end
