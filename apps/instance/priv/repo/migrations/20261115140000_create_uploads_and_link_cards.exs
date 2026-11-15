defmodule Lumia.Repo.Migrations.CreateUploadsAndLinkCards do
  @moduledoc """
  Attachments and link cards (specs/media.md). `uploads` is one row per
  client-encrypted blob: who uploaded it, for which conversation and role,
  how long it is, and the message that named it once one did (for a
  personal conversation, the `SendEncrypted` command's id). A message keeps
  its attachments and cards as their encoded protos, as the event it was
  delivered in does. `link_cards` is the instance's card cache, and what
  owns a card's picture.
  """

  use Ecto.Migration

  def change do
    create table(:uploads) do
      # No foreign keys: a row is what finds its object in the blob store,
      # so it goes only when `Lumia.Uploads` removes both.
      add :uploader_account_id, :uuid, null: false
      add :channel_id, :uuid, null: false
      add :role, :string, null: false
      add :size, :bigint, null: false
      add :stored, :boolean, null: false, default: false
      add :message_id, :uuid
      # Dropped from its message by an edit: no longer served, and gone
      # with the message.
      add :removed, :boolean, null: false, default: false
      add :created_at, :utc_datetime_usec, null: false
    end

    create index(:uploads, [:message_id], where: "message_id IS NOT NULL")
    create index(:uploads, [:uploader_account_id])
    create index(:uploads, [:created_at], where: "message_id IS NULL")

    alter table(:messages) do
      add :attachments, {:array, :binary}, null: false, default: []
      add :cards, {:array, :binary}, null: false, default: []
      add :card_ids, {:array, :uuid}, null: false, default: []
      # The links whose cards it wants, in order, fetched or not yet.
      add :card_urls, {:array, :text}, null: false, default: []
    end

    create index(:messages, [:card_ids], using: :gin, where: "card_ids <> '{}'")

    create table(:link_cards) do
      add :url, :text, null: false
      # The encoded Lumia.V1.Card, or nil when the page gave none.
      add :card, :binary
      add :image_address, :binary
      add :fetched_by_account_id, references(:accounts, on_delete: :nilify_all)
      add :fetched_at, :utc_datetime_usec, null: false
    end

    create index(:link_cards, [:url, :fetched_at])
    create index(:link_cards, [:fetched_by_account_id, :fetched_at])
    create index(:link_cards, [:fetched_at])
  end
end
