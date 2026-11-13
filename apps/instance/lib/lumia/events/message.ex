defmodule Lumia.Events.Message do
  @moduledoc "Materialized message, built from message events."

  use Lumia.Schema

  schema "messages" do
    field :channel_id, Ecto.UUID
    field :seq, :integer
    field :author_account_id, Ecto.UUID
    field :parent_id, Ecto.UUID
    field :body, :string
    field :created_at, :utc_datetime_usec
    field :edited_at, :utc_datetime_usec
    field :deleted_at, :utc_datetime_usec
    # A notice (architecture 6.8): "joined", "added", "left" or "removed",
    # about `notice_account_id`; the author is who did it.
    field :notice, :string
    field :notice_account_id, Ecto.UUID
    # A quote reply (architecture 6.6): the original, its author and the
    # text quoted, which goes when the original does.
    field :reply_to_id, Ecto.UUID
    field :reply_to_author_id, Ecto.UUID
    field :reply_text, :string
    field :reply_deleted, :boolean, default: false
    # Whom the message notifies as a mention: the accounts it names and the
    # author it quotes, and whether it names everyone.
    field :mentioned_account_ids, {:array, Ecto.UUID}, default: []
    field :mentions_everyone, :boolean, default: false
    field :mention_spans, {:array, :map}, default: []
  end
end
