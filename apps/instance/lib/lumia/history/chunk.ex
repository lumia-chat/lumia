defmodule Lumia.History.Chunk do
  @moduledoc "An encrypted history chunk a device wrote (e2ee spec section 9). Opaque to the instance."

  use Lumia.Schema

  @primary_key false
  schema "history_chunks" do
    field :identity_id, :binary, primary_key: true
    field :chunk_id, Ecto.UUID, primary_key: true
    field :device_id, Ecto.UUID
    field :channel_id, Ecto.UUID
    field :ciphertext, :binary
    field :nonce, :binary
    field :size, :integer
    field :created_at, :utc_datetime_usec
  end
end
