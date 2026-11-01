defmodule Lumia.Events.Message do
  @moduledoc "Materialized message, built from message events."

  use Lumia.Schema

  schema "messages" do
    field :channel_id, Ecto.UUID
    field :seq, :integer
    field :author_account_id, Ecto.UUID
    field :parent_id, Ecto.UUID
    field :body, :string
    field :created_at, :utc_datetime_usec
    field :edited_at, :utc_datetime_usec
    field :deleted_at, :utc_datetime_usec
  end
end
