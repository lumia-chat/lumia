defmodule Lumia.Repo.Migrations.MediaSweepsAndReservations do
  @moduledoc """
  What the review of attachments and link cards found missing
  (specs/media.md 4.4 and 6.4):

    * `uploads.deleting`: a blob whose removal has begun. The row stays
      until the store has let the object go, so a store that failed is
      asked again by the next sweep.
    * `link_cards.pending`: a fetch under way. It counts against its
      account's unfurls a minute from the moment it starts, and is no one's
      cache until it is done.
    * `messages.revision_seqs`: the events that carried a message's
      content after it was created (edits, cards set), whose payloads a
      delete purges with the original's.
    * An index on deleted messages, which the hourly sweep reads.
  """

  use Ecto.Migration

  def change do
    alter table(:uploads) do
      add :deleting, :boolean, null: false, default: false
    end

    create index(:uploads, [:id], where: "deleting", name: :uploads_deleting_index)

    alter table(:link_cards) do
      add :pending, :boolean, null: false, default: false
    end

    alter table(:messages) do
      add :revision_seqs, {:array, :bigint}, null: false, default: []
    end

    create index(:messages, [:deleted_at], where: "deleted_at IS NOT NULL")
  end
end
