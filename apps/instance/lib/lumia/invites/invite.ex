defmodule Lumia.Invites.Invite do
  @moduledoc """
  An invite link's row (architecture 6.6): the code the link carries, the
  server it joins, the channel when it is a connected-channel invite, its
  expiry and use count, and whether joiners become guests.
  """

  use Lumia.Schema

  schema "invites" do
    field :code, :string
    field :server_id, Ecto.UUID
    field :channel_id, Ecto.UUID
    field :created_by, Ecto.UUID
    field :expires_at, :utc_datetime_usec
    field :max_uses, :integer, default: 0
    field :uses, :integer, default: 0
    field :as_guest, :boolean, default: false
    field :guest_days, :integer
    timestamps()
  end
end
