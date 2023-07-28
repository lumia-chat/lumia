defmodule Lumia.Repo.Migrations.CreateChats do
  use Ecto.Migration

  def change do
    create table(:chats) do
      add :name, :string, null: false
      add :description, :string
      add :is_private, :boolean, default: false, null: false

      timestamps()
    end
  end
end
