defmodule Lumia.Repo.Migrations.CreateInvites do
  @moduledoc "Invite links (architecture 6.1 and 6.6): a code, its server, its limits, its uses."

  use Ecto.Migration

  def change do
    create table(:invites) do
      add :code, :text, null: false
      add :server_id, references(:servers, on_delete: :delete_all), null: false
      add :channel_id, references(:channels, on_delete: :delete_all)
      add :created_by, references(:accounts, on_delete: :delete_all), null: false
      add :expires_at, :utc_datetime_usec
      # 0 is unlimited.
      add :max_uses, :integer, null: false, default: 0
      add :uses, :integer, null: false, default: 0
      add :as_guest, :boolean, null: false, default: false
      add :guest_days, :integer
      timestamps()
    end

    create unique_index(:invites, [:code])
    create index(:invites, [:server_id])
  end
end
