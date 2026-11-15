defmodule Lumia.Workers.Unfurl do
  @moduledoc """
  Fetches the pages a server message wants cards for and the cache did not
  have when it was sent or edited, then puts the cards on the message
  (`Lumia.Events.set_cards/2`, which appends `MessageCardsSet`). Sending
  never waits for a page (specs/media.md 6.4). One attempt: a page that
  fails is cached as having no card, and the message goes without.
  """

  use Oban.Worker, queue: :default, max_attempts: 1

  alias Lumia.Cards
  alias Lumia.Events
  alias Lumia.Events.Message
  alias Lumia.Repo

  @impl Oban.Worker
  def perform(%Oban.Job{args: %{"channel_id" => channel_id, "message_id" => message_id}}) do
    case Repo.get_by(Message, id: message_id, channel_id: channel_id) do
      %Message{deleted_at: nil, card_urls: urls} ->
        Cards.fetch_missing(urls)
        Events.set_cards(channel_id, message_id)

      _ ->
        :ok
    end
  end
end
