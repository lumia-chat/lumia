defmodule Lumia.Events.Command do
  @moduledoc """
  An accepted command, kept for idempotent replay (architecture 8.3).

  `fingerprint` is a hash of the operation so a retry with the same
  `command_id` can be told apart from a different command reusing the id.
  """

  use Lumia.Schema

  @primary_key false
  schema "commands" do
    field :channel_id, Ecto.UUID, primary_key: true
    field :command_id, Ecto.UUID, primary_key: true
    field :actor_account_id, Ecto.UUID
    field :fingerprint, :binary
    field :seq, :integer
    field :event_id, Ecto.UUID
    field :created_at, :utc_datetime_usec
  end
end
