defmodule Lumia.Dms.Request do
  @moduledoc "A DM request: `state` is `pending`, `accepted`, `declined` or `blocked`."

  use Lumia.Schema

  schema "dm_requests" do
    field :from_account_id, Ecto.UUID
    field :to_account_id, Ecto.UUID
    field :state, :string, default: "pending"
    field :created_at, :utc_datetime_usec
    field :responded_at, :utc_datetime_usec
  end
end
