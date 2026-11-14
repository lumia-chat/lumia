defmodule Lumia.Repo.Migrations.AddShowDeletedToServers do
  @moduledoc "Whether a server's deleted messages keep their place as \"(deleted)\" (architecture 6.7); off by default."

  use Ecto.Migration

  def change do
    alter table(:servers) do
      add :show_deleted, :boolean, null: false, default: false
    end
  end
end
