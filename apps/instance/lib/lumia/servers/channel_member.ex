defmodule Lumia.Servers.ChannelMember do
  @moduledoc """
  An account in a channel (architecture 6.8): what puts the channel in its
  sidebar, delivers it and lets it post there. A server DM's participants
  are its members.
  """

  use Lumia.Schema

  @primary_key false
  schema "channel_members" do
    field :channel_id, Ecto.UUID, primary_key: true
    field :account_id, Ecto.UUID, primary_key: true
    field :added_by_account_id, Ecto.UUID
    field :joined_at, :utc_datetime_usec
  end
end
