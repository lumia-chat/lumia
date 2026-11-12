defmodule Lumia.Blobs.Blob do
  @moduledoc """
  A stored blob's row: where it is in the blob store (`address`, which is
  also its name there), what it is, and its content key wrapped by the
  instance master key. See `Lumia.Blobs`.
  """

  use Lumia.Schema

  @primary_key {:address, :binary, autogenerate: false}
  schema "blobs" do
    field :kind, :string
    field :media_type, :string
    field :size, :integer
    field :wrapped_key, :binary, redact: true
    field :owner_account_id, Ecto.UUID
    timestamps(updated_at: false)
  end
end
