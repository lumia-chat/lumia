defmodule Lumia.Repo.Migrations.CreateAccountRails do
  @moduledoc "An account's rail arrangement (architecture 6.9): its servers in order and its folders, one row per account."

  use Ecto.Migration

  def change do
    create table(:account_rails, primary_key: false) do
      add :account_id, references(:accounts, on_delete: :delete_all), primary_key: true

      # The encoded `lumia.v1.Rail` as last written, tidied.
      add :rail, :binary, null: false
      add :updated_at, :utc_datetime_usec, null: false
    end
  end
end
