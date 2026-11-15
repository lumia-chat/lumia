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

  The instance fetches nothing unless its operator turned cards on
  (`LUMIA_LINK_CARDS=true`, architecture hard rule 3): until then every
  link has no card, and messages go without.
  """

  import Ecto.Query

  alias Lumia.Authz.Access
  alias Lumia.Blobs
  alias Lumia.Cards.Fetch
  alias Lumia.Cards.Head
  alias Lumia.Cards.LinkCard
  alias Lumia.Events.Message
  alias Lumia.Repo
  alias Lumia.Tx
  alias Lumia.V1

  @max_cards 3
  @fresh_seconds 3_600
  @none_seconds 600
  @per_minute 30
  @deleted_days 30

  @doc "Whether the instance fetches link cards: only when its operator said so."
  def enabled?, do: Application.get_env(:lumia, :link_cards, false) == true

  @doc """
  The card for `url` in a server channel where the account may send:
  `{:ok, %V1.Card{} | nil}` from the cache or a fetch now (always nil
  while cards are off), `{:error, :rate_limited}` past #{@per_minute}
  fetches a minute, or the channel's access error. Personal conversations
  never come here: the instance must not learn what is linked in them.
  """
  def unfurl(account_id, channel_id, url) do
    with {:ok, access} <- Access.require(account_id, channel_id, :send_messages),
         true <- not Lumia.Dms.personal_kind?(access.channel.kind) || {:error, :not_found},
         true <- link?(url) || {:error, {:invalid_argument, "url"}} do
      if enabled?(), do: cached_or_fetched(url, account_id), else: {:ok, nil}
    end
  end

  defp cached_or_fetched(url, account_id) do
    case cached(url) do
      %LinkCard{} = row -> {:ok, card(row)}
      nil -> with {:ok, row} <- fetch(url, account_id), do: {:ok, card(row)}
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

  # As the core's message parser finds links, which may hold what a URL
  # cannot as written (`Fetch.parse/1` encodes it).
  defp link?(url), do: byte_size(url) <= 2_048 and Fetch.parse(url) != :error

  @doc """
  The cards a message gets now for `urls`, in their order: the ones it
  already has (`kept`, a list of `{url, card, id}`) and the cache's fresh
  ones. `{cards, card_ids, missing}`: `missing` are the links to fetch
  after the commit. A page cached as having no card gives none, and while
  cards are off no link has one.
  """
  def for_message(urls, kept \\ []) do
    kept = Map.new(kept, fn {url, card, id} -> {url, {card, id}} end)
    lookup = if enabled?(), do: &cached_pair/1, else: fn _url -> :none end

    {found, missing} =
      Enum.reduce(urls, {[], []}, fn url, {found, missing} ->
        case Map.get(kept, url) || lookup.(url) do
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
  Fetches the `urls` of a message by `account_id` that have no fresh row
  (for `Lumia.Workers.Unfurl`), so that `for_message/2` finds them all: at
  once, each against the author's fetches a minute as a preview is. One
  past the rate is left, and its link goes without a card.
  """
  def fetch_missing(urls, account_id) do
    if enabled?() do
      urls
      |> Enum.filter(&(cached(&1) == nil))
      |> Task.async_stream(&fetch(&1, account_id),
        max_concurrency: @max_cards,
        timeout: 15_000,
        on_timeout: :kill_task
      )
      |> Stream.run()
    end

    :ok
  end

  # The newest finished row for the link still within its time.
  defp cached(url) do
    now = DateTime.utc_now()

    Repo.one(
      from c in LinkCard,
        where:
          c.url == ^url and not c.pending and
            ((not is_nil(c.card) and c.fetched_at > ^DateTime.add(now, -@fresh_seconds)) or
               (is_nil(c.card) and c.fetched_at > ^DateTime.add(now, -@none_seconds))),
        order_by: [desc: c.fetched_at],
        limit: 1
    )
  end

  # Fetches the page and its picture and keeps what came of it, card or
  # none. The card's url is the link as written, whatever it redirected to.
  # The page and the picture share one deadline (specs/media.md 6.3).
  defp fetch(url, account_id) do
    with {:ok, row} <- reserve(url, account_id) do
      deadline = Fetch.deadline()

      draft =
        with {:ok, final_url, html} <- fetcher().page(url, deadline),
             %{} = draft <- Head.parse(html, final_url) do
          draft
        else
          _ -> nil
        end

      {card, image_address} =
        case draft do
          nil -> {nil, nil}
          draft -> with_image(%{draft | url: url}, deadline)
        end

      {:ok,
       row
       |> Ecto.Changeset.change(
         card: card && V1.Card.encode(card),
         image_address: image_address,
         pending: false,
         fetched_at: DateTime.utc_now()
       )
       |> Repo.update!()}
    end
  end

  # Starts a fetch for the account when it has room: its fetches of the
  # last minute, those under way included, are counted and the new one
  # written under a lock on the account, so requests made at once cannot
  # all find room.
  defp reserve(url, account_id) do
    Tx.transact(fn ->
      Repo.query!("SELECT pg_advisory_xact_lock($1)", [:erlang.phash2({__MODULE__, account_id})])
      since = DateTime.add(DateTime.utc_now(), -60)

      count =
        Repo.aggregate(
          from(c in LinkCard,
            where: c.fetched_by_account_id == ^account_id and c.fetched_at > ^since
          ),
          :count
        )

      if count < @per_minute do
        {:ok,
         Repo.insert!(%LinkCard{
           url: url,
           pending: true,
           fetched_by_account_id: account_id,
           fetched_at: DateTime.utc_now()
         })}
      else
        {:error, :rate_limited}
      end
    end)
  end

  defp with_image(draft, deadline) do
    card = %V1.Card{
      url: draft.url,
      title: draft.title,
      description: draft.description,
      site_name: draft.site_name,
      color: draft.color,
      video: draft.video
    }

    with true <- draft.image_url != "" and Blobs.enabled?(),
         {:ok, bytes} <- fetcher().image(draft.image_url, deadline),
         {:ok, {media_type, encoded, width, height}} <- picture(bytes, deadline),
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

  # The instance does a few pictures at once (`Lumia.Media`); a card waits
  # for its turn within its deadline rather than go without for an hour.
  defp picture(bytes, deadline) do
    case Lumia.Media.card(bytes) do
      {:error, :rate_limited} = refused ->
        if System.monotonic_time(:millisecond) + 50 < deadline do
          Process.sleep(50)
          picture(bytes, deadline)
        else
          refused
        end

      result ->
        result
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
  pictures, a message deleted more than #{@deleted_days} days ago naming
  none (specs/media.md 6.4: its cards go with it). A row whose picture the
  store would not let go stays for the next sweep. Returns how many went.
  """
  def sweep do
    now = DateTime.utc_now()
    before = DateTime.add(now, -@fresh_seconds)
    deleted_before = DateTime.add(now, -@deleted_days, :day)

    # `card_ids <> '{}'` written out, so the partial GIN index serves the
    # containment test.
    stale =
      Repo.all(
        from c in LinkCard,
          as: :card,
          where:
            c.fetched_at < ^before and
              not exists(
                from m in Message,
                  where:
                    fragment("? <> '{}'", m.card_ids) and
                      fragment("? @> ARRAY[?]::uuid[]", m.card_ids, parent_as(:card).id) and
                      (is_nil(m.deleted_at) or m.deleted_at >= ^deleted_before)
              ),
          limit: 1_000
      )

    gone =
      Enum.filter(stale, fn row ->
        row.image_address == nil or Blobs.delete(row.image_address) == :ok
      end)

    Repo.delete_all(from c in LinkCard, where: c.id in ^Enum.map(gone, & &1.id))
    length(gone)
  end
end
