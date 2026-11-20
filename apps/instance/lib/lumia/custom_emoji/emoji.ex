defmodule Lumia.CustomEmoji.Emoji do
  @moduledoc """
  One of a server's custom emoji (specs/emoji.md 2). A removed one keeps
  its row, with `deleted_at` set and its picture gone (`image_url` nil),
  so that reactions with it can still be named.
  """

  use Lumia.Schema

  schema "custom_emoji" do
    field :server_id, Ecto.UUID
    field :name, :string
    field :image_url, :string
    field :animated, :boolean, default: false
    field :width, :integer
    field :height, :integer
    field :created_by_account_id, Ecto.UUID
    field :deleted_at, :utc_datetime_usec
    timestamps()
  end
end
