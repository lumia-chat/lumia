defmodule Lumia.Repo.Migrations.CreateRepliesReactionsReads do
  @moduledoc """
  Quote replies, reactions, mentions and read state (architecture 6.6).

  A message keeps what it quotes (the original's id and author and the
  text quoted, dropped when the original is deleted) and whom it notifies
  as a mention (the accounts it names, the author it quotes, and whether
  it names everyone). Reactions are rows, one per account, emoji and
  message. Read state is one row per account and conversation: the last
  event read.

  Everyone in a conversation starts with it read to its head, so this
  conversion leaves nothing unread that was there before it.
  """

  use Ecto.Migration

  def up do
    alter table(:messages) do
      add :reply_to_id, :binary_id
      add :reply_to_author_id, :binary_id
      add :reply_text, :text
      add :reply_deleted, :boolean, null: false, default: false
      add :mentioned_account_ids, {:array, :binary_id}, null: false, default: []
      add :mentions_everyone, :boolean, null: false, default: false
      # The mentions as the message gives them, with where each is in its
      # body, for drawing: [{"account_id" | "everyone", "start", "end"}].
      add :mention_spans, {:array, :map}, null: false, default: []
    end

    create index(:messages, [:reply_to_id], where: "reply_to_id IS NOT NULL")

    create index(:messages, [:mentioned_account_ids],
             using: "gin",
             where: "mentioned_account_ids <> '{}'"
           )

    create table(:message_reactions, primary_key: false) do
      add :message_id, :binary_id, null: false, primary_key: true

      add :account_id, references(:accounts, on_delete: :delete_all),
        null: false,
        primary_key: true

      add :reaction, :text, null: false, primary_key: true
      add :channel_id, references(:channels, on_delete: :delete_all), null: false
      add :created_at, :utc_datetime_usec, null: false
    end

    create index(:message_reactions, [:channel_id, :message_id])

    create table(:read_positions, primary_key: false) do
      add :account_id, references(:accounts, on_delete: :delete_all),
        null: false,
        primary_key: true

      add :channel_id, references(:channels, on_delete: :delete_all),
        null: false,
        primary_key: true

      add :seq, :bigint, null: false, default: 0
      add :updated_at, :utc_datetime_usec, null: false
    end

    execute """
    INSERT INTO read_positions (account_id, channel_id, seq, updated_at)
    SELECT cm.account_id, cm.channel_id, c.last_seq, now()
    FROM channel_members cm JOIN channels c ON c.id = cm.channel_id
    ON CONFLICT DO NOTHING
    """

    execute """
    INSERT INTO read_positions (account_id, channel_id, seq, updated_at)
    SELECT p.account_id, p.channel_id, c.last_seq, now()
    FROM dm_participants p JOIN channels c ON c.id = p.channel_id
    ON CONFLICT DO NOTHING
    """
  end

  def down do
    drop table(:read_positions)
    drop table(:message_reactions)
    drop index(:messages, [:mentioned_account_ids])
    drop index(:messages, [:reply_to_id])

    alter table(:messages) do
      remove :reply_to_id
      remove :reply_to_author_id
      remove :reply_text
      remove :reply_deleted
      remove :mentioned_account_ids
      remove :mentions_everyone
      remove :mention_spans
    end
  end
end
