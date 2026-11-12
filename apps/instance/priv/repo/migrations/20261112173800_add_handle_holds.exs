defmodule Lumia.Repo.Migrations.AddHandleHolds do
  @moduledoc """
  The handle an account last left (`PATCH /account`), kept from everyone
  else until `until`: one row per account, so a rename holds one handle
  and no more.
  """

  use Ecto.Migration

  def change do
    create table(:handle_holds, primary_key: false) do
      add :account_id, references(:accounts, on_delete: :delete_all), primary_key: true
      add :handle, :text, null: false
      add :until, :utc_datetime_usec, null: false
    end

    create unique_index(:handle_holds, ["lower(handle)"], name: :handle_holds_handle_index)
  end
end
