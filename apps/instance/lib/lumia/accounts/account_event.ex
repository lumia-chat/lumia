defmodule Lumia.Accounts.AccountEvent do
  @moduledoc "One entry in an account log (docs/specs/delivery.md section 2)."

  use Lumia.Schema

  @primary_key false
  schema "account_events" do
    field :account_id, Ecto.UUID, primary_key: true
    field :seq, :integer, primary_key: true
    field :id, Ecto.UUID
    field :kind, :string
    field :created_at, :utc_datetime_usec
  end
end
