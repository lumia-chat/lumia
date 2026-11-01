defmodule Lumia.Events.Channel do
  @moduledoc "A channel row: owner of the per-channel sequence."

  use Lumia.Schema

  schema "channels" do
    field :kind, :string, default: "text"
    field :last_seq, :integer, default: 0
    # Second granularity; see the migration for why.
    field :last_seq_at, :utc_datetime
    field :acl_version, :integer, default: 1
    timestamps()
  end
end
