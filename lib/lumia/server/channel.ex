defmodule Lumia.Server.Channel do
  use Lumia.Schema

  import Ecto.Changeset

  alias Lumia.Accounts
  alias Lumia.Server

  schema "channels" do
    field :description, :string
    field :is_private, :boolean, default: false
    field :name, :string

    has_many :messages, Server.Message
    has_many :user_channels, Accounts.UserChannel
    has_many :users, through: [:user_channels, :user]

    timestamps()
  end

  @doc false
  def changeset(channel, attrs) do
    channel
    |> cast(attrs, [:name, :description, :is_private])
    |> validate_required([:name, :description])
  end
end
