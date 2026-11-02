defmodule Lumia.Mls.GroupInfo do
  @moduledoc "The newest GroupInfo per conversation, by the epoch the publisher states (e2ee spec section 6)."

  use Lumia.Schema

  @primary_key false
  schema "mls_group_info" do
    field :channel_id, Ecto.UUID, primary_key: true
    field :epoch, :integer
    field :group_info, :binary
    field :updated_at, :utc_datetime_usec
  end
end
