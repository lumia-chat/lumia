defmodule Lumia.CardsTest do
  @moduledoc """
  Link cards in server content (specs/media.md 6.4): the preview, the
  cache, cards put on a message at once or after the commit, edits, and
  the fetcher's refusals.
  """

  use Lumia.DataCase, async: false
  use Oban.Testing, repo: Lumia.Repo

  import Lumia.Fixtures

  alias Lumia.Cards
  alias Lumia.Cards.Fetch
  alias Lumia.Events
  alias Lumia.Events.Message
  alias Lumia.FakePages
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.Servers
  alias Lumia.V1
  alias Lumia.Workers.Unfurl

  setup do
    Application.put_env(:lumia, :card_fetch, FakePages)
    FakePages.reset()
    on_exit(fn -> Application.delete_env(:lumia, :card_fetch) end)

    owner = account("owner")
    {:ok, server} = Servers.create_server(owner.id, "Acme")
    {:ok, general} = Servers.create_channel(server.id, owner.id, "general")
    %{owner: owner, general: general}
  end

  defp send(channel, account, body, card_urls) do
    command = send_message(channel.id, body)
    {:send_message, op} = command.op
    command = %{command | op: {:send_message, %{op | card_urls: card_urls}}}
    {Events.append(channel.id, actor(account), command), Proto.from_uuid(command.command_id)}
  end

  defp last_event(channel), do: Events.events_after(channel.id, 0, 200) |> List.last()

  test "a preview is fetched once and then comes from the cache", ctx do
    url = "https://example.com/a"
    assert {:ok, %V1.Card{} = card} = Cards.unfurl(ctx.owner.id, ctx.general.id, url)
    assert card.title == "An & example"
    assert card.description == "What it is about"
    assert card.url == url
    assert "/api/v1/cards/" <> name = card.image_url

    assert {:ok, %{media_type: "image/png"}} =
             Lumia.Blobs.fetch(Lumia.Blobs.address(name), "card")

    assert {:ok, ^card} = Cards.unfurl(ctx.owner.id, ctx.general.id, url)
    assert FakePages.count() == 1

    # A page with no card is cached as none.
    assert {:ok, nil} = Cards.unfurl(ctx.owner.id, ctx.general.id, "https://elsewhere.test/")
    assert {:ok, nil} = Cards.unfurl(ctx.owner.id, ctx.general.id, "https://elsewhere.test/")
    assert FakePages.count() == 2

    assert {:error, {:invalid_argument, "url"}} =
             Cards.unfurl(ctx.owner.id, ctx.general.id, "ftp://example.com/")
  end

  test "never in a personal conversation" do
    %{channel: dm, a: a} = personal_dm()
    assert {:error, :not_found} = Cards.unfurl(a.id, dm.id, "https://example.com/a")
  end

  test "a cached card goes on the message as it is sent", ctx do
    url = "https://example.com/b"
    {:ok, _} = Cards.unfurl(ctx.owner.id, ctx.general.id, url)
    {{:ok, _}, message_id} = send(ctx.general, ctx.owner, "see #{url}", [url])

    %V1.Event{payload: {:message_created, %V1.MessageCreated{message: message}}} =
      last_event(ctx.general)

    assert [%V1.Card{url: ^url}] = message.cards
    assert [%V1.Card{}] = Proto.message(Repo.get(Message, message_id)).cards
    refute_enqueued(worker: Unfurl)
  end

  test "one not cached yet arrives after the commit, as MessageCardsSet", ctx do
    url = "https://example.com/c"
    {{:ok, _}, message_id} = send(ctx.general, ctx.owner, "see #{url}", [url])

    %V1.Event{payload: {:message_created, %V1.MessageCreated{message: message}}} =
      last_event(ctx.general)

    assert message.cards == []
    assert_enqueued(worker: Unfurl, args: %{channel_id: ctx.general.id, message_id: message_id})

    assert :ok = perform_job(Unfurl, %{channel_id: ctx.general.id, message_id: message_id})

    %V1.Event{payload: {:message_cards_set, set}} = last_event(ctx.general)
    assert set.message_id == Proto.uuid(message_id)
    assert [%V1.Card{url: ^url, title: "An & example"}] = set.cards

    # Run again, it changes nothing and says nothing.
    seq = last_event(ctx.general).seq
    assert :ok = perform_job(Unfurl, %{channel_id: ctx.general.id, message_id: message_id})
    assert last_event(ctx.general).seq == seq
  end

  test "a card is asked for only for a link the message holds, three at most", ctx do
    assert {{:error, {:invalid_argument, "card_urls"}}, _} =
             send(ctx.general, ctx.owner, "nothing linked", ["https://example.com/d"])

    links = for n <- 1..4, do: "https://example.com/#{n}"

    assert {{:error, {:invalid_argument, "card_urls"}}, _} =
             send(ctx.general, ctx.owner, Enum.join(links, " "), links)
  end

  test "an edit keeps a card, drops one whose link went, and the author can drop one", ctx do
    url = "https://example.com/e"
    {:ok, _} = Cards.unfurl(ctx.owner.id, ctx.general.id, url)
    {{:ok, _}, message_id} = send(ctx.general, ctx.owner, "see #{url}", [url])

    edit = fn body, cards ->
      %V1.Command{
        command_id: Proto.uuid(Ecto.UUID.generate(version: 7)),
        channel_id: Proto.uuid(ctx.general.id),
        op:
          {:edit_message,
           %V1.EditMessage{message_id: Proto.uuid(message_id), body: body, cards: cards}}
      }
    end

    assert {:ok, _} = Events.append(ctx.general.id, actor(ctx.owner), edit.("still #{url}", nil))
    %V1.Event{payload: {:message_edited, edited}} = last_event(ctx.general)
    assert [%V1.Card{url: ^url}] = edited.cards

    assert {:ok, _} =
             Events.append(
               ctx.general.id,
               actor(ctx.owner),
               edit.("still #{url}", %V1.CardUrls{urls: []})
             )

    %V1.Event{payload: {:message_edited, edited}} = last_event(ctx.general)
    assert edited.cards == []
    assert Repo.get(Message, message_id).card_urls == []
  end

  test "cards past their hour that no message names are swept with their pictures", ctx do
    {:ok, _} = Cards.unfurl(ctx.owner.id, ctx.general.id, "https://example.com/kept")
    {:ok, _} = Cards.unfurl(ctx.owner.id, ctx.general.id, "https://example.com/gone")

    {{:ok, _}, _} =
      send(ctx.general, ctx.owner, "https://example.com/kept", ["https://example.com/kept"])

    long_ago = DateTime.add(DateTime.utc_now(), -2, :hour)
    Repo.update_all(Lumia.Cards.LinkCard, set: [fetched_at: long_ago])
    assert Cards.sweep() == 1
    assert [%{url: "https://example.com/kept"}] = Repo.all(Lumia.Cards.LinkCard)
  end

  test "the fetcher refuses what it must not reach" do
    for url <- [
          "http://127.0.0.1/",
          "http://localhost/",
          "http://10.0.0.1/",
          "http://169.254.169.254/latest/meta-data/",
          "http://[::1]/",
          "http://[::ffff:127.0.0.1]/",
          "https://example.com:8443/",
          "file:///etc/passwd",
          "gopher://example.com/"
        ] do
      assert {:error, reason} = Fetch.page(url)
      assert reason in [:refused, :nxdomain], "#{url} gave #{inspect(reason)}"
    end
  end
end
