defmodule Lumia.Servers.ChannelOverwrite do
  @moduledoc """
  A per-channel permission overwrite for a role or a member. A bit set in
  both `allow` and `deny` is treated as deny.
  """

  use Lumia.Schema

  @type t :: %__MODULE__{}

  @primary_key false
  schema "channel_overwrites" do
    field :channel_id, Ecto.UUID, primary_key: true
    field :target_kind, :string, primary_key: true
    field :target_id, Ecto.UUID, primary_key: true
    field :allow, :integer, default: 0
    field :deny, :integer, default: 0
  end
end
