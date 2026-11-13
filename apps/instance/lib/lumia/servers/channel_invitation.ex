defmodule Lumia.Servers.ChannelInvitation do
  @moduledoc """
  An add that waits for its member's yes, because they take adds to
  channels as invitations (architecture 6.8).
  """

  use Lumia.Schema

  @primary_key false
  schema "channel_invitations" do
    field :channel_id, Ecto.UUID, primary_key: true
    field :account_id, Ecto.UUID, primary_key: true
    field :invited_by_account_id, Ecto.UUID
    field :created_at, :utc_datetime_usec
  end
end
