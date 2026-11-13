defmodule Lumia.Servers.Member do
  @moduledoc """
  An account's membership in a server. `kind` is `member` or `guest`
  (architecture 6.2); guest status lives here, not on the account.
  """

  use Lumia.Schema

  @primary_key false
  schema "server_members" do
    field :server_id, Ecto.UUID, primary_key: true
    field :account_id, Ecto.UUID, primary_key: true
    field :kind, :string, default: "member"
    field :nickname, :string
    field :joined_at, :utc_datetime_usec
    field :expires_at, :utc_datetime_usec
    # The member's own switches in this server (architecture 6.8): adds to
    # channels become invitations, and nobody may start a server DM with them.
    field :channel_adds_by_invitation, :boolean, default: false
    field :server_dms_off, :boolean, default: false
  end
end
