defmodule Lumia.Reads.ReadPosition do
  @moduledoc "Where an account has read a conversation to: its last event read (architecture 6.6)."

  use Lumia.Schema

  @primary_key false
  schema "read_positions" do
    field :account_id, Ecto.UUID, primary_key: true
    field :channel_id, Ecto.UUID, primary_key: true
    field :seq, :integer, default: 0
    field :updated_at, :utc_datetime_usec
  end
end
