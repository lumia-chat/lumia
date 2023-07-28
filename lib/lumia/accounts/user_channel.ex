defmodule Lumia.Accounts.UserChannel do
  use Lumia.Schema

  import Ecto.Changeset

  alias Lumia.Accounts
  alias Lumia.Server

  schema "user_channels" do
    belongs_to :user, Accounts.User
    belongs_to :channel, Server.Channel

    timestamps(updated_at: false)
  end

  @doc false
  def changeset(user_channel, attrs) do
    user_channel
    |> cast(attrs, [])
    |> validate_required([])
  end
end
