defmodule Lumia.Repo.Migrations.CreateDmRequests do
  @moduledoc """
  DM requests (architecture 6.4): the gate for a personal DM between two
  accounts that are not friends. One pending request per direction.
  """

  use Ecto.Migration

  def change do
    create table(:dm_requests) do
      add :from_account_id, references(:accounts, on_delete: :delete_all), null: false
      add :to_account_id, references(:accounts, on_delete: :delete_all), null: false
      add :state, :text, null: false, default: "pending"
      add :created_at, :utc_datetime_usec, null: false
      add :responded_at, :utc_datetime_usec
    end

    create unique_index(:dm_requests, [:from_account_id, :to_account_id],
             where: "state = 'pending'",
             name: :dm_requests_pending_index
           )

    create index(:dm_requests, [:to_account_id])
  end
end
