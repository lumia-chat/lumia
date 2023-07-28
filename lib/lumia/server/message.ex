defmodule Lumia.Server.Message do
  use Lumia.Schema

  import Ecto.Changeset

  alias Lumia.Accounts
  alias Lumia.Server

  schema "messages" do
    field :text, :string

    belongs_to :chat, Server.Chat
    belongs_to :user, Accounts.User

    timestamps(type: :utc_datetime_usec)
  end

  @doc false
  def changeset(message, attrs) do
    message
    |> cast(attrs, [:text])
    |> validate_required([:text])
    |> validate_length(:text, max: 600)
  end
end
