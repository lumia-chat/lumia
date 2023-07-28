defmodule Lumia.Server.Chat do
  use Lumia.Schema

  import Ecto.Changeset

  alias Lumia.Server

  schema "chats" do
    field :description, :string
    field :is_private, :boolean, default: false
    field :name, :string

    has_many :messages, Server.Message

    timestamps()
  end

  @doc false
  def changeset(chat, attrs) do
    chat
    |> cast(attrs, [:name, :description, :is_private])
    |> validate_required([:name, :description])
  end
end
