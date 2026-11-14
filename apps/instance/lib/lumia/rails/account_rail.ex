defmodule Lumia.Rails.AccountRail do
  @moduledoc "An account's rail arrangement as last written, tidied (architecture 6.9)."

  use Lumia.Schema

  @primary_key false
  schema "account_rails" do
    field :account_id, Ecto.UUID, primary_key: true
    # The encoded `Lumia.V1.Rail`.
    field :rail, :binary
    field :updated_at, :utc_datetime_usec
  end
end
