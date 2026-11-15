defmodule Lumia.Uploads.Upload do
  @moduledoc """
  One client-encrypted blob (specs/media.md 4): its uploader, the
  conversation it is for, its role (`"display"`, `"thumbnail"` or
  `"file"`), the ciphertext's length, whether the bytes have arrived, and
  the message that named it. Its id is its name in the blob store.
  """

  use Lumia.Schema

  schema "uploads" do
    field :uploader_account_id, Ecto.UUID
    field :channel_id, Ecto.UUID
    field :role, :string
    field :size, :integer
    field :stored, :boolean, default: false
    field :message_id, Ecto.UUID
    field :removed, :boolean, default: false
    field :created_at, :utc_datetime_usec
  end
end
