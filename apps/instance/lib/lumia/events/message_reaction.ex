defmodule Lumia.Events.MessageReaction do
  @moduledoc """
  One account's reaction to a message: an emoji (architecture 6.6). For one
  of the server's custom emoji, `emoji_id` is set and `reaction` is that id
  as text, so a rename keeps its reactions together (specs/emoji.md 9).
  """

  use Lumia.Schema

  @primary_key false
  schema "message_reactions" do
    field :message_id, Ecto.UUID, primary_key: true
    field :account_id, Ecto.UUID, primary_key: true
    field :reaction, :string, primary_key: true
    field :channel_id, Ecto.UUID
    field :emoji_id, Ecto.UUID
    field :created_at, :utc_datetime_usec
  end
end
