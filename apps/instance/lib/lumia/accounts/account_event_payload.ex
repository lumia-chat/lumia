defmodule Lumia.Accounts.AccountEventPayload do
  @moduledoc "The purgeable half of an account event."

  use Lumia.Schema

  @type t :: %__MODULE__{}

  @primary_key false
  schema "account_event_payloads" do
    field :account_id, Ecto.UUID, primary_key: true
    field :seq, :integer, primary_key: true
    field :payload, :map
  end
end
