defmodule Lumia.Server.Message do
  use Lumia.Schema

  import Ecto.Changeset

  alias Lumia.Accounts
  alias Lumia.Server

  schema "messages" do
    field :text, :string

    belongs_to :channel, Server.Channel
    belongs_to :user, Accounts.User

    timestamps(type: :utc_datetime_usec)
  end

  @doc false
  def changeset(message, attrs) do
    message
    |> cast(attrs, [:text])
    # |> update_change(:text, &(&1 && String.trim_leading(&1)))
    |> validate_required([:text])
    |> validate_length(:text, max: 600)
    |> html_escape(:text)
  end

  defp html_escape(changeset, field) do
    update_change(changeset, field, fn value ->
      {:safe, data} = Phoenix.HTML.html_escape(value)
      IO.iodata_to_binary(data)
    end)
  end
end
