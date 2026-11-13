defmodule Lumia.Events.MessageReaction do
  @moduledoc "One account's reaction to a message: an emoji (architecture 6.6)."

  use Lumia.Schema

  @primary_key false
  schema "message_reactions" do
    field :message_id, Ecto.UUID, primary_key: true
    field :account_id, Ecto.UUID, primary_key: true
    field :reaction, :string, primary_key: true
    field :channel_id, Ecto.UUID
    field :created_at, :utc_datetime_usec
  end
end
