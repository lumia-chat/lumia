defmodule Lumia.Cards do
  @moduledoc """
  Link cards in server content (specs/media.md 6.4), built by the
  instance: members' clients ask for a preview while composing
  (`unfurl/3`), a message names the links whose cards were kept, and the
  instance puts the cards on it, from its cache at once or, for a page
  not fetched yet, after the commit (`Lumia.Workers.Unfurl` appends
  `MessageCardsSet`). A member cannot forge a card, and no site sees a
  member's address.

  Each fetch is a `link_cards` row: the cache for an hour (ten minutes for
  a page that gave no card), and afterwards the owner of its picture for
  as long as a message names it.
  """

  import Ecto.Query

  alias Lumia.Authz.Access
  alias Lumia.Blobs
  alias Lumia.Cards.Fetch
  alias Lumia.Cards.Head
  alias Lumia.Cards.LinkCard
  alias Lumia.Events.Message
  alias Lumia.Repo
  alias Lumia.V1

  @max_cards 3
  @fresh_seconds 3_600
  @none_seconds 600
  @per_minute 30

  @doc """
  The card for `url` in a server channel where the account may send:
  `{:ok, %V1.Card{} | nil}` from the cache or a fetch now, `{:error,
  :rate_limited}` past #{@per_minute} fetches a minute, or the channel's
  access error. Personal conversations never come here: the instance must
  not learn what is linked in them.
  """
  def unfurl(account_id, channel_id, url) do
    with {:ok, access} <- Access.require(account_id, channel_id, :send_messages),
         true <- not Lumia.Dms.personal_kind?(access.channel.kind) || {:error, :not_found},
         true <- link?(url) || {:error, {:invalid_argument, "url"}} do
      cached_or_fetched(url, account_id)
    end
  end

  defp cached_or_fetched(url, account_id) do
    case cached(url) do
      %LinkCard{} = row -> {:ok, card(row)}
      nil -> with :ok <- within_rate(account_id), do: {:ok, url |> fetch(account_id) |> card()}
    end
  end

  @doc """
  The links of a message whose cards it asks for, checked (specs/media.md
  6.4): at most #{@max_cards}, each `http(s)` and in the body; duplicates
  dropped. `{:ok, urls}` or `{:error, {:invalid_argument, "card_urls"}}`.
  """
  def check_urls(body, urls) do
    urls = Enum.uniq(urls)

    if length(urls) <= @max_cards and Enum.all?(urls, &(link?(&1) and String.contains?(body, &1))),
      do: {:ok, urls},
      else: {:error, {:invalid_argument, "card_urls"}}
  end

  defp link?(url) do
    byte_size(url) <= 2_048 and
      match?(
        {:ok, %URI{scheme: s, host: h}} when s in ["http", "https"] and h not in [nil, ""],
        URI.new(url)
      )
  end

  @doc """
  The cards a message gets now for `urls`, in their order: the ones it
  already has (`kept`, a list of `{url, card, id}`) and the cache's fresh
  ones. `{cards, card_ids, missing}`: `missing` are the links to fetch
  after the commit. A page cached as having no card gives none.
  """
  def for_message(urls, kept \\ []) do
    kept = Map.new(kept, fn {url, card, id} -> {url, {card, id}} end)

    {found, missing} =
      Enum.reduce(urls, {[], []}, fn url, {found, missing} ->
        case Map.get(kept, url) || cached_pair(url) do
          nil -> {found, [url | missing]}
          :none -> {found, missing}
          pair -> {[pair | found], missing}
        end
      end)

    found = Enum.reverse(found)
    {Enum.map(found, &elem(&1, 0)), Enum.map(found, &elem(&1, 1)), Enum.reverse(missing)}
  end

  defp cached_pair(url) do
    case cached(url) do
      nil -> nil
      %LinkCard{card: nil} -> :none
      %LinkCard{} = row -> {card(row), row.id}
    end
  end

  @doc """
  Fetches `urls` that have no fresh row (for `Lumia.Workers.Unfurl`), so
  that `for_message/2` finds them all.
  """
  def fetch_missing(urls) do
    Enum.each(urls, fn url -> if cached(url) == nil, do: fetch(url, nil) end)
  end

  # The newest row for the link still within its time.
  defp cached(url) do
    now = DateTime.utc_now()

    Repo.one(
      from c in LinkCard,
        where:
          c.url == ^url and
            ((not is_nil(c.card) and c.fetched_at > ^DateTime.add(now, -@fresh_seconds)) or
               (is_nil(c.card) and c.fetched_at > ^DateTime.add(now, -@none_seconds))),
        order_by: [desc: c.fetched_at],
        limit: 1
    )
  end

  defp within_rate(account_id) do
    since = DateTime.add(DateTime.utc_now(), -60)

    count =
      Repo.aggregate(
        from(c in LinkCard,
          where: c.fetched_by_account_id == ^account_id and c.fetched_at > ^since
        ),
        :count
      )

    if count < @per_minute, do: :ok, else: {:error, :rate_limited}
  end

  # Fetches the page and its picture and keeps what came of it, card or
  # none. The card's url is the link as written, whatever it redirected to.
  defp fetch(url, account_id) do
    draft =
      with {:ok, final_url, html} <- fetcher().page(url),
           %{} = draft <- Head.parse(html, final_url) do
        draft
      else
        _ -> nil
      end

    {card, image_address} =
      case draft do
        nil -> {nil, nil}
        draft -> with_image(%{draft | url: url})
      end

    Repo.insert!(%LinkCard{
      url: url,
      card: card && V1.Card.encode(card),
      image_address: image_address,
      fetched_by_account_id: account_id,
      fetched_at: DateTime.utc_now()
    })
  end

  defp with_image(draft) do
    card = %V1.Card{
      url: draft.url,
      title: draft.title,
      description: draft.description,
      site_name: draft.site_name,
      color: draft.color,
      video: draft.video
    }

    with true <- draft.image_url != "" and Blobs.enabled?(),
         {:ok, bytes} <- fetcher().image(draft.image_url),
         {:ok, {media_type, encoded, width, height}} <- Lumia.Media.card(bytes),
         {:ok, blob} <- Blobs.put(encoded, "card", media_type, nil) do
      {%{
         card
         | image_url: "/api/v1/cards/" <> Blobs.name(blob.address),
           image_width: width,
           image_height: height
       }, blob.address}
    else
      _ -> {card, nil}
    end
  end

  # `Lumia.Cards.Fetch`; tests name a stand-in, since it refuses to
  # connect anywhere a test's own server could be.
  defp fetcher, do: Application.get_env(:lumia, :card_fetch, Fetch)

  defp card(nil), do: nil
  defp card(%LinkCard{card: nil}), do: nil
  defp card(%LinkCard{card: bytes}), do: V1.Card.decode(bytes)

  @doc """
  Removes cache rows past their hour that no message names, with their
  pictures. Returns how many went.
  """
  def sweep do
    before = DateTime.add(DateTime.utc_now(), -@fresh_seconds)

    stale =
      Repo.all(
        from c in LinkCard,
          as: :card,
          where:
            c.fetched_at < ^before and
              not exists(
                from m in Message,
                  where: fragment("? @> ARRAY[?]::uuid[]", m.card_ids, parent_as(:card).id)
              ),
          limit: 1_000
      )

    Enum.each(stale, fn row ->
      if row.image_address, do: Blobs.delete(row.image_address)
    end)

    Repo.delete_all(from c in LinkCard, where: c.id in ^Enum.map(stale, & &1.id))
    length(stale)
  end
end
