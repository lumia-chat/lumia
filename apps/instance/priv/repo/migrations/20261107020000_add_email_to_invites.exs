defmodule Lumia.Repo.Migrations.AddEmailToInvites do
  @moduledoc "The address an email invite went to (architecture 6.2)."

  use Ecto.Migration

  def change do
    alter table(:invites) do
      add :email, :string
    end
  end
end
