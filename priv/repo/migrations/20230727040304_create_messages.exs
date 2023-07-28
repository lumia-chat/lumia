defmodule Lumia.Repo.Migrations.CreateMessages do
  use Ecto.Migration

  def change do
    create table(:messages) do
      add :text, :text, null: false
      add :chat_id, references(:chats, on_delete: :delete_all), null: false
      add :user_id, references(:users, on_delete: :nilify_all)
      timestamps(type: :utc_datetime_usec)
    end

    create index(:messages, [:chat_id])
    create index(:messages, [:user_id])
  end
end
