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
  end
end
