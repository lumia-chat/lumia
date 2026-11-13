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
  end
end
