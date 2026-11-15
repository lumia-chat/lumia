defmodule Lumia.Cards.LinkCard do
  @moduledoc """
  One fetch of a link's page (specs/media.md 6.4): the card it gave, as an
  encoded `Lumia.V1.Card` (nil when the page gave none), the address of its
  picture in the blob store, who asked and when. Rows are the cache for an
  hour and then stay as long as a message's `card_ids` name them, since
  they own the picture.
  """

  use Lumia.Schema

  schema "link_cards" do
    field :url, :string
    field :card, :binary
    field :image_address, :binary
    field :fetched_by_account_id, Ecto.UUID
    field :fetched_at, :utc_datetime_usec
    # A fetch under way: counted against its account's rate, cached for
    # nobody until it is done.
    field :pending, :boolean, default: false
  end
end
