defmodule Lumia.Servers.ChannelViewer do
  @moduledoc "An account the viewer-diff job last found able to view a channel."

  use Lumia.Schema

  @primary_key false
  schema "channel_viewers" do
    field :channel_id, Ecto.UUID, primary_key: true
    field :account_id, Ecto.UUID, primary_key: true
  end
end
