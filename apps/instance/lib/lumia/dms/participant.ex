defmodule Lumia.Dms.Participant do
  @moduledoc "An account in a PERSONAL_* channel. Participants are the channel's whole ACL."

  use Lumia.Schema

  @primary_key false
  schema "dm_participants" do
    field :channel_id, Ecto.UUID, primary_key: true
    field :account_id, Ecto.UUID, primary_key: true
  end
end
