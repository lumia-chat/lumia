defmodule Lumia.Repo.Migrations.ServerStorage do
  @moduledoc """
  A server's storage (specs/media.md 4.5): each blob names the server its
  channel is in, the server keeps the sum of its blobs' lengths, and two
  settings say how long attachments are kept and what happens when the
  server is full. Blobs already kept are counted here.
  """

  use Ecto.Migration

  def up do
    alter table(:uploads) do
      # Nil for a personal conversation's.
      add :server_id, :uuid
    end

    alter table(:servers) do
      # Every blob of the server not yet being removed, in bytes.
      add :stored_bytes, :bigint, null: false, default: 0
      add :attachment_retention_days, :integer
      add :storage_full, :string, null: false, default: "refuse"
    end

    create index(:uploads, [:server_id], where: "server_id IS NOT NULL AND NOT deleting")

    execute """
    UPDATE uploads u SET server_id = c.server_id
    FROM channels c WHERE c.id = u.channel_id AND c.server_id IS NOT NULL
    """

    execute """
    UPDATE servers s SET stored_bytes = t.total
    FROM (SELECT server_id, sum(size) AS total FROM uploads
          WHERE server_id IS NOT NULL AND NOT deleting GROUP BY server_id) t
    WHERE t.server_id = s.id
    """
  end

  def down do
    drop index(:uploads, [:server_id])

    alter table(:servers) do
      remove :stored_bytes
      remove :attachment_retention_days
      remove :storage_full
    end

    alter table(:uploads) do
      remove :server_id
    end
  end
end
