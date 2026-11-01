defmodule Lumia.Events.Event do
  @moduledoc """
  One entry in a channel log. Primary key is `(channel_id, seq)`; `id` is the
  server-minted UUIDv7 clients deduplicate by.
  """

  use Lumia.Schema

  @primary_key false
  schema "events" do
    field :channel_id, Ecto.UUID, primary_key: true
    field :seq, :integer, primary_key: true
    field :id, Ecto.UUID
    field :kind, :string
    field :actor_account_id, Ecto.UUID
    field :actor_device_id, Ecto.UUID
    field :command_id, Ecto.UUID
    field :acl_version, :integer
    field :created_at, :utc_datetime_usec
  end
end
