defmodule Lumia.Reads do
  @moduledoc """
  Read state (architecture 6.6): for each account and conversation, the
  last event read, and from it what is unread and what of that notifies
  the account.

  A conversation's position moves only forward. Moving it appends
  `ReadPositionUpdated` to the account's log, which its other devices
  follow, and wakes those that are not connected, to take away the
  notifications of what is now read and set their badge.

  What counts: in a server's channel, the messages others sent after the
  position (not notices, not deleted, not thread replies), and of those,
  the ones that notify the account (a mention of it or of everyone, a
  quote of its message); in a DM, server or personal, every message counts
  as both. A personal DM's messages are its encrypted application events,
  counted by their place in the log with nothing decrypted.

  Everyone enters a conversation with it read to its head (`start/2`), so
  what was said before they came is not unread.
  """

  import Ecto.Query

  alias Lumia.Accounts
  alias Lumia.Authz.Access
  alias Lumia.Dms.Participant
  alias Lumia.Events.Channel
  alias Lumia.Events.Event
  alias Lumia.Events.Message
  alias Lumia.Proto
  alias Lumia.Reads.ReadPosition
  alias Lumia.Repo
  alias Lumia.Servers.ChannelViewer
  alias Lumia.Tx
  alias Lumia.V1

  @mentions_page 50

  @doc """
  The account enters a conversation read to its head: called as it joins,
  is added, starts a DM or is put in one. A position already held stays.
  """
  def start(account_id, channel_id) do
    head = Repo.one(from c in Channel, where: c.id == ^channel_id, select: c.last_seq) || 0

    Repo.insert(
      %ReadPosition{
        account_id: account_id,
        channel_id: channel_id,
        seq: head,
        updated_at: DateTime.utc_now()
      },
      on_conflict: :nothing
    )

    :ok
  end

  @doc """
  The account has read a conversation it can see through `seq`, capped at
  the conversation's head. Returns `{:ok, %V1.ReadState{}}` as it then
  stands; `{:error, :not_found}` for a conversation it cannot see.
  """
  def mark(account_id, channel_id, seq) when is_integer(seq) and seq >= 0 do
    with {:ok, access} <- Access.viewable_channel(account_id, channel_id),
         seq = min(seq, access.channel.last_seq),
         {:ok, _} <- Tx.transact(fn -> move(account_id, channel_id, seq, DateTime.utc_now()) end) do
      state = List.first(states(account_id, [channel_id]))
      {:ok, state || %V1.ReadState{channel_id: Proto.uuid(channel_id), read_seq: seq}}
    end
  end

  def mark(_account_id, _channel_id, _seq), do: {:error, :invalid_argument}

  # Forward only: the row moves when `seq` is past it, and then the
  # account's log says so and its other devices are woken.
  defp move(account_id, channel_id, seq, now) do
    {_, rows} =
      Repo.insert_all(
        ReadPosition,
        [%{account_id: account_id, channel_id: channel_id, seq: seq, updated_at: now}],
        on_conflict:
          from(r in ReadPosition,
            where: r.seq < ^seq,
            update: [set: [seq: ^seq, updated_at: ^now]]
          ),
        conflict_target: [:account_id, :channel_id],
        returning: [:seq]
      )

    if rows == [], do: {:ok, false}, else: announce(account_id, channel_id, seq)
  end

  defp announce(account_id, channel_id, seq) do
    moved = %V1.ReadPositionUpdated{channel_id: Proto.uuid(channel_id), seq: seq}

    with {:ok, _} <- Accounts.append_event(account_id, {:read_position_updated, moved}) do
      Tx.after_commit(fn -> Lumia.Push.read_elsewhere(account_id) end)
      {:ok, true}
    end
  end

  @doc "The read state of every conversation the account is shown, for its snapshot."
  def states(account_id), do: states(account_id, shown(account_id))

  @doc "The read state of the account in each of `channel_ids`."
  def states(_account_id, []), do: []

  def states(account_id, channel_ids) do
    positions =
      Repo.all(
        from r in ReadPosition,
          where: r.account_id == ^account_id and r.channel_id in ^channel_ids,
          select: {r.channel_id, r.seq}
      )
      |> Map.new()

    kinds =
      Repo.all(from c in Channel, where: c.id in ^channel_ids, select: {c.id, c.kind})
      |> Map.new()

    counted = counts(account_id, channel_ids, kinds)

    for id <- channel_ids, Map.has_key?(kinds, id) do
      {unread, mentions} = Map.get(counted, id, {0, 0})

      %V1.ReadState{
        channel_id: Proto.uuid(id),
        read_seq: Map.get(positions, id, 0),
        unread: unread,
        mentions: mentions
      }
    end
  end

  @doc """
  The account's badge: what notifies it and is unread, every DM message
  among it, across everything it is shown.
  """
  def badge(account_id) do
    account_id
    |> states()
    |> Enum.reduce(0, &(&1.mentions + &2))
  end

  # The conversations an account is shown: its servers' channels it is in
  # and can see (the viewer diff's rows), and its personal DMs.
  defp shown(account_id) do
    server =
      Repo.all(from v in ChannelViewer, where: v.account_id == ^account_id, select: v.channel_id)

    personal =
      Repo.all(from p in Participant, where: p.account_id == ^account_id, select: p.channel_id)

    Enum.uniq(server ++ personal)
  end

  defp counts(account_id, channel_ids, kinds) do
    {personal, server} = Enum.split_with(channel_ids, &Lumia.Dms.personal_kind?(kinds[&1]))
    dms = for id <- server, kinds[id] == "server_dm", into: MapSet.new(), do: id
    Map.merge(message_counts(account_id, server, dms), event_counts(account_id, personal))
  end

  defp message_counts(_account_id, [], _dms), do: %{}

  defp message_counts(account_id, channel_ids, dms) do
    from(m in Message,
      join: r in ReadPosition,
      on: r.channel_id == m.channel_id and r.account_id == ^account_id,
      where:
        m.channel_id in ^channel_ids and m.seq > r.seq and m.author_account_id != ^account_id and
          is_nil(m.notice) and is_nil(m.deleted_at) and is_nil(m.parent_id),
      group_by: m.channel_id,
      select:
        {m.channel_id, count(m.id),
         fragment(
           "count(*) FILTER (WHERE ? OR ? = ANY(?))",
           m.mentions_everyone,
           type(^account_id, Ecto.UUID),
           m.mentioned_account_ids
         )}
    )
    |> Repo.all()
    |> Map.new(fn {id, unread, mentions} ->
      # Every message of a server DM is for the account.
      {id, {unread, if(MapSet.member?(dms, id), do: unread, else: mentions)}}
    end)
  end

  defp event_counts(_account_id, []), do: %{}

  defp event_counts(account_id, channel_ids) do
    from(e in Event,
      join: r in ReadPosition,
      on: r.channel_id == e.channel_id and r.account_id == ^account_id,
      where:
        e.channel_id in ^channel_ids and e.seq > r.seq and e.kind == "encrypted" and
          e.actor_account_id != ^account_id,
      group_by: e.channel_id,
      select: {e.channel_id, count(e.seq)}
    )
    |> Repo.all()
    |> Map.new(fn {id, unread} -> {id, {unread, unread}} end)
  end

  @doc """
  The messages that notify the account (a mention of it or of everyone, a
  quote of its message), newest first, in the channels it is shown of
  `server_id` or of every server when it is nil; `before` is the last
  message id of the page before. Returns `{messages, has_more}` as
  `Lumia.V1.Message`s with their reactions as the account sees them.
  """
  def mentions(account_id, server_id, before \\ nil, limit \\ @mentions_page) do
    limit = min(max(limit, 1), @mentions_page)

    query =
      from m in Message,
        where:
          m.channel_id in subquery(shown_in(account_id, server_id)) and
            m.author_account_id != ^account_id and is_nil(m.deleted_at) and
            (m.mentions_everyone or
               fragment("? = ANY(?)", type(^account_id, Ecto.UUID), m.mentioned_account_ids)),
        order_by: [desc: m.created_at, desc: m.id],
        limit: ^(limit + 1)

    rows = query |> after_message(before) |> Repo.all()
    page = Enum.take(rows, limit)
    reactions = Lumia.Snapshots.reactions(Enum.map(page, & &1.id), account_id)
    {Enum.map(page, &Proto.message(&1, Map.get(reactions, &1.id, []))), length(rows) > limit}
  end

  # The server channels the account is shown, of one server or of all.
  defp shown_in(account_id, server_id) do
    channels =
      from v in ChannelViewer,
        join: c in Channel,
        on: c.id == v.channel_id,
        where: v.account_id == ^account_id and not is_nil(c.server_id),
        select: v.channel_id

    if server_id, do: where(channels, [_v, c], c.server_id == ^server_id), else: channels
  end

  defp after_message(query, nil), do: query

  defp after_message(query, before) do
    case Repo.get(Message, before) do
      %Message{created_at: at, id: id} ->
        where(query, [m], m.created_at < ^at or (m.created_at == ^at and m.id < ^id))

      nil ->
        query
    end
  end
end
