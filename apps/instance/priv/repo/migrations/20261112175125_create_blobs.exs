defmodule Lumia.Repo.Migrations.CreateBlobs do
  @moduledoc """
  What the instance knows of each stored blob (architecture 12): its
  address in the blob store, the content key wrapped by the instance
  master key, and who it belongs to. The bytes themselves are in the blob
  store, encrypted under the content key.
  """

  use Ecto.Migration

  def change do
    create table(:blobs, primary_key: false) do
      add :address, :binary, primary_key: true
      add :kind, :text, null: false
      add :media_type, :text, null: false
      add :size, :bigint, null: false
      add :wrapped_key, :binary, null: false
      add :owner_account_id, references(:accounts, on_delete: :nilify_all)
      timestamps(updated_at: false)
    end

    create index(:blobs, [:owner_account_id])
  end
end
