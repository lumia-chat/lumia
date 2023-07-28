defmodule Lumia.Repo.Migrations.CreateChannels do
  use Ecto.Migration

  def change do
    create table(:channels) do
      add :name, :string, null: false
      add :description, :string
      add :is_private, :boolean, default: false, null: false

      timestamps()
    end
  end
end
