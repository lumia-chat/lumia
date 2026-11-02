defmodule Lumia.Events.EventPayload do
  @moduledoc """
  The purgeable half of an event. Retention and deletion null `payload`;
  the `Lumia.Events.Event` row stays so ordering and gap detection hold.
  """

  use Lumia.Schema

  @primary_key false
  schema "event_payloads" do
    field :channel_id, Ecto.UUID, primary_key: true
    field :seq, :integer, primary_key: true
    field :payload, :binary
  end
end
