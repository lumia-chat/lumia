defmodule Lumia.Events do
  @moduledoc """
  The event log: append commands to channels, assign `seq`, keep acks for
  idempotent replay, broadcast after commit.

  This is the write path from docs/specs/delivery.md sections 3, 4 and 9.
  One transaction per command: lock the channel row by incrementing
  `last_seq`, insert the event row, its serialized `Lumia.V1.Event` payload
  and the command record, materialize the message, commit. Then, and only
  then, broadcast the same bytes on `channel:<id>`.

  A stored payload is the exact bytes sent on the wire, so an event is
  encoded once. A null payload (purged) reads back as `Redacted`.
  """

  import Ecto.Query

  alias Lumia.Dms
  alias Lumia.Events.Channel
  alias Lumia.Events.Command
  alias Lumia.Events.Event
  alias Lumia.Events.EventPayload
  alias Lumia.Events.Message
  alias Lumia.Events.MessageReaction
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.Schema
  alias Lumia.Tx
  alias Lumia.V1

  # The most of a message a quote keeps, in characters (architecture 6.6).
  @quote_max 2_000

  @doc "Creates a channel. Spike-level; the real context lives elsewhere later."
  def create_channel(kind \\ "text") do
    Repo.insert(%Channel{kind: kind})
  end

  @doc """
  Appends a command to a channel log on behalf of `actor`, a map with
  `:account_id` and `:device_id` (string UUIDs; device may be nil).

  Returns the ack for the committed event. A replay of an already accepted
  command (same id, same fingerprint) returns the original ack without
  writing. The same id with a different operation is `{:error, :conflict}`.
  Broadcasts after commit unless `broadcast: false`.
  """
  def append(channel_id, actor, %V1.Command{} = command, opts \\ []) do
    case Proto.from_uuid(command.command_id) do
      nil -> {:error, :invalid_argument}
      command_id -> append(channel_id, actor, command, command_id, opts)
    end
  end

  defp append(channel_id, actor, command, command_id, opts) do
    fingerprint = fingerprint(actor, command)

    Tx.transact(fn ->
      case Repo.get_by(Command, channel_id: channel_id, command_id: command_id) do
        %Command{fingerprint: ^fingerprint} = existing -> {:ok, {ack(existing), nil}}
        %Command{} -> {:error, :conflict}
        nil -> write(channel_id, actor, command, command_id, fingerprint)
      end
    end)
    |> settle(channel_id, actor, command, {command_id, fingerprint}, opts)
  end

  defp settle({:ok, {ack, nil}}, _channel_id, _actor, _command, _key, _opts), do: {:ok, ack}

  defp settle({:ok, {ack, {event, bytes}}}, channel_id, _actor, _command, _key, opts) do
    broadcast_after_commit(channel_id, event, bytes, opts)
    Tx.after_commit(fn -> Lumia.Push.after_commit(channel_id, event) end)

    {:ok, ack}
  end

  # A concurrent identical command won the race on the unique key. The
  # whole transaction rolled back, so the seq it took is gone too.
  defp settle(
         {:error, %Ecto.Changeset{errors: [command_id: {_, [constraint: :unique] ++ _}]}},
         channel_id,
         actor,
         command,
         _key,
         opts
       ),
       do: append(channel_id, actor, command, opts)

  defp settle(
         {:error, :conflict},
         channel_id,
         _actor,
         _command,
         {command_id, fingerprint},
         _opts
       ),
       do: settle_conflict(channel_id, command_id, fingerprint)

  defp settle(other, _channel_id, _actor, _command, _key, _opts), do: other

  # The materialized row collided (its id is the command id). Either a
  # concurrent identical command committed first, in which case its ack is
  # the answer (delivery spec 12: "both callers receive the same ack"), or
  # the commands row was purged and this is a late retry: a conflict.
  defp settle_conflict(channel_id, command_id, fingerprint) do
    case Repo.get_by(Command, channel_id: channel_id, command_id: command_id) do
      %Command{fingerprint: ^fingerprint} = existing -> {:ok, ack(existing)}
      _ -> {:error, :conflict}
    end
  end

  @doc """
  Appends a server-originated event (ACL change, purge marker, membership)
  to a channel log. `payload` is a `Lumia.V1.Event` payload tuple such as
  `{:acl_changed, %Lumia.V1.AclChanged{}}`. No command, no idempotency row,
  no materialization. Broadcasts after commit.
  """
  def append_system(channel_id, {tag, _} = payload, opts \\ []) when is_atom(tag) do
    result =
      Tx.transact(fn ->
        now = DateTime.utc_now()

        with {:ok, seq, acl_version, _kind} <- take_head(channel_id, now) do
          event = %V1.Event{
            event_id: Proto.uuid(Schema.generate_id()),
            channel_id: Proto.uuid(channel_id),
            seq: seq,
            created_at: Proto.timestamp(now),
            acl_version: acl_version,
            payload: payload
          }

          insert_event(channel_id, event, nil, nil, now)
        end
      end)

    with {:ok, {event, bytes}} <- result do
      broadcast_after_commit(channel_id, event, bytes, opts)
      {:ok, %{channel_id: channel_id, seq: event.seq, event_id: Proto.from_uuid(event.event_id)}}
    end
  end

  @doc """
  Posts a notice in a server channel (architecture 6.8): a message with no
  body, written by `author_id`, about `account_id`; `kind` is `"joined"`,
  `"added"`, `"left"` or `"removed"`. It is materialized as any message
  is, so it is in snapshots and scrollback, and cannot be edited or
  deleted. Broadcasts after commit.
  """
  def append_notice(channel_id, author_id, kind, account_id)
      when kind in ~w(joined added left removed) do
    message_id = Schema.generate_id()

    result =
      Tx.transact(fn ->
        now = DateTime.utc_now()

        with {:ok, seq, acl_version, _kind} <- take_head(channel_id, now),
             message = %V1.Message{
               message_id: Proto.uuid(message_id),
               channel_id: Proto.uuid(channel_id),
               seq: seq,
               author_account_id: Proto.uuid(author_id),
               created_at: Proto.timestamp(now),
               notice: Proto.notice(kind, account_id)
             },
             event = %V1.Event{
               event_id: Proto.uuid(Schema.generate_id()),
               channel_id: Proto.uuid(channel_id),
               seq: seq,
               actor_account_id: Proto.uuid(author_id),
               created_at: Proto.timestamp(now),
               acl_version: acl_version,
               payload: {:message_created, %V1.MessageCreated{message: message}}
             },
             {:ok, {event, bytes}} <-
               insert_event(channel_id, event, nil, %{account_id: author_id}, now),
             {:ok, _} <-
               Repo.insert(%Message{
                 id: message_id,
                 channel_id: channel_id,
                 seq: seq,
                 author_account_id: author_id,
                 body: "",
                 created_at: now,
                 notice: kind,
                 notice_account_id: account_id
               }) do
          {:ok, {event, bytes}}
        end
      end)

    with {:ok, {event, bytes}} <- result do
      broadcast_after_commit(channel_id, event, bytes, [])
      {:ok, %{channel_id: channel_id, seq: event.seq, message_id: message_id}}
    end
  end

  # Deferred until the outermost transaction commits (see Lumia.Tx).
  defp broadcast_after_commit(channel_id, event, bytes, opts) do
    if Keyword.get(opts, :broadcast, true),
      do: Tx.after_commit(fn -> broadcast(channel_id, event, bytes) end)
  end

  @doc """
  Retention: purges every payload at or below the newest event created
  before `cutoff`, deletes the materialized messages, and appends one
  `PayloadsPurged{before_seq}` event (delivery section 10). Returns the
  number of payloads nulled; zero means nothing was old enough and no
  marker was appended. Idempotent.
  """
  def purge_before(channel_id, %DateTime{} = cutoff) do
    Tx.transact(fn ->
      newest_old =
        Repo.one(
          from e in Event,
            where: e.channel_id == ^channel_id and e.created_at < ^cutoff,
            select: max(e.seq)
        )

      case newest_old do
        nil -> {:ok, 0}
        seq -> purge_through(channel_id, seq)
      end
    end)
  end

  defp purge_through(channel_id, seq) do
    {nulled, _} =
      Repo.update_all(
        from(p in EventPayload,
          where: p.channel_id == ^channel_id and p.seq <= ^seq and not is_nil(p.payload)
        ),
        set: [payload: nil]
      )

    if nulled == 0 do
      {:ok, 0}
    else
      drop_quotes(
        channel_id,
        from(m in Message, where: m.channel_id == ^channel_id and m.seq <= ^seq, select: m.id)
      )

      Repo.delete_all(from m in Message, where: m.channel_id == ^channel_id and m.seq <= ^seq)

      with {:ok, _} <-
             append_system(
               channel_id,
               {:payloads_purged, %V1.PayloadsPurged{before_seq: seq + 1}}
             ) do
        {:ok, nulled}
      end
    end
  end

  @doc "Events after `after_seq`, in order, as `Lumia.V1.Event` structs. The sync query."
  def events_after(channel_id, after_seq, limit \\ 200) do
    from(e in Event,
      left_join: p in EventPayload,
      on: p.channel_id == e.channel_id and p.seq == e.seq,
      where: e.channel_id == ^channel_id and e.seq > ^after_seq,
      order_by: [asc: e.seq],
      limit: ^limit,
      select: {e, p.payload}
    )
    |> Repo.all()
    |> Enum.map(fn {row, bytes} -> to_proto(row, bytes) end)
  end

  @doc "The current head of a channel log as `{last_seq, acl_version}`, or nil."
  def head(channel_id) do
    Repo.one(from c in Channel, where: c.id == ^channel_id, select: {c.last_seq, c.acl_version})
  end

  @doc """
  Channels whose head moved at or after `since`. The reconciliation query.

  Inclusive, and `last_seq_at` has second granularity, so a channel written
  in the same second as `since` is returned again. Spurious nudges are
  harmless (architecture 8.2); missed ones are not.
  """
  def changed_since(%DateTime{} = since) do
    since = DateTime.truncate(since, :second)

    Repo.all(
      from c in Channel,
        where: c.last_seq_at >= ^since,
        select: {c.id, c.last_seq}
    )
  end

  @doc "The PubSub topic for a channel log."
  def topic(channel_id), do: "channel:" <> channel_id

  @doc """
  Broadcasts a committed event: the struct and its serialized bytes, so
  protobuf sessions forward the bytes untouched (delivery section 4).
  """
  def broadcast(channel_id, %V1.Event{} = event, bytes) when is_binary(bytes) do
    Phoenix.PubSub.broadcast(Lumia.PubSub, topic(channel_id), {:event, event, bytes})
  end

  @doc "Builds the wire struct for a stored event row; a null payload is Redacted."
  def to_proto(%Event{} = row, nil) do
    %V1.Event{
      event_id: Proto.uuid(row.id),
      channel_id: Proto.uuid(row.channel_id),
      seq: row.seq,
      command_id: Proto.uuid(row.command_id),
      actor_account_id: Proto.uuid(row.actor_account_id),
      actor_device_id: Proto.uuid(row.actor_device_id),
      created_at: Proto.timestamp(row.created_at),
      acl_version: row.acl_version,
      payload: {:redacted, %V1.Redacted{was: redacted_kind(row.kind)}}
    }
  end

  def to_proto(%Event{}, bytes) when is_binary(bytes), do: V1.Event.decode(bytes)

  # ---- Write path --------------------------------------------------------------

  defp write(channel_id, actor, command, command_id, fingerprint) do
    now = DateTime.utc_now()

    with {:ok, seq, acl_version, kind} <- take_head(channel_id, now),
         :ok <- check_kind(kind, command),
         :ok <- advance_epoch(channel_id, command),
         {:ok, command} <- resolve(channel_id, command),
         {:ok, payload} <- payload_for(command, actor, channel_id, seq, now),
         event = build_event(channel_id, seq, acl_version, actor, command, payload, now),
         {:ok, {event, bytes}} <- insert_event(channel_id, event, command_id, actor, now),
         {:ok, _} <- materialize(channel_id, seq, actor, command, now),
         {:ok, record} <- insert_command(channel_id, command_id, actor, fingerprint, event, now) do
      {:ok, {ack(record), {event, bytes}}}
    end
  end

  defp build_event(channel_id, seq, acl_version, actor, command, payload, now) do
    %V1.Event{
      event_id: Proto.uuid(Schema.generate_id()),
      channel_id: Proto.uuid(channel_id),
      seq: seq,
      command_id: command.command_id,
      actor_account_id: Proto.uuid(actor.account_id),
      actor_device_id: Proto.uuid(Map.get(actor, :device_id)),
      created_at: Proto.timestamp(now),
      acl_version: acl_version,
      payload: payload
    }
  end

  # Locks the row for the rest of the transaction and assigns seq in one
  # statement. Everything after this is serialized per channel.
  # last_seq_at is truncated to the second so that writes within the same
  # second do not change an indexed column and the row update stays HOT.
  defp take_head(channel_id, now) do
    head_at = DateTime.truncate(now, :second)

    query =
      from(c in Channel,
        where: c.id == ^channel_id,
        select: {c.last_seq, c.acl_version, c.kind}
      )

    case Repo.update_all(query, inc: [last_seq: 1], set: [last_seq_at: head_at]) do
      {0, _} -> {:error, :channel_not_found}
      {1, [{seq, acl_version, kind}]} -> {:ok, seq, acl_version, kind}
    end
  end

  # Encrypted channels take only SendEncrypted, and nothing else takes it
  # (docs/protocol.md section 15).
  defp check_kind(kind, %V1.Command{op: {:send_encrypted, _}}) do
    if Dms.personal_kind?(kind), do: :ok, else: {:error, :invalid_command}
  end

  defp check_kind(kind, %V1.Command{}) do
    if Dms.personal_kind?(kind), do: {:error, :invalid_command}, else: :ok
  end

  # A CONTROL event is a commit and carries the epoch it produces; it is
  # accepted only at the conversation's current epoch plus one, which
  # serializes concurrent commits (e2ee spec section 4): the second of two
  # commits for the same epoch finds the conversation already there and is
  # refused. The channel row is already locked by take_head, so the
  # conditional update is the whole check. A WELCOME carries the epoch its
  # commit produced, which must be the current one, and advances nothing.
  defp advance_epoch(channel_id, %V1.Command{
         op:
           {:send_encrypted, %V1.SendEncrypted{payload: %V1.EncryptedPayload{kind: :CONTROL} = p}}
       }) do
    previous = p.epoch - 1

    query = from(c in Channel, where: c.id == ^channel_id and c.mls_epoch == ^previous)

    case Repo.update_all(query, set: [mls_epoch: p.epoch]) do
      {1, _} -> :ok
      {0, _} -> {:error, {:failed_precondition, "epoch"}}
    end
  end

  defp advance_epoch(channel_id, %V1.Command{
         op:
           {:send_encrypted, %V1.SendEncrypted{payload: %V1.EncryptedPayload{kind: :WELCOME} = p}}
       }) do
    current = Repo.one(from c in Channel, where: c.id == ^channel_id, select: c.mls_epoch)
    if current == p.epoch, do: :ok, else: {:error, {:failed_precondition, "epoch"}}
  end

  defp advance_epoch(_channel_id, _command), do: :ok

  defp insert_event(channel_id, %V1.Event{} = event, command_id, actor, now) do
    bytes = V1.Event.encode(event)

    with {:ok, _} <-
           Repo.insert(%Event{
             channel_id: channel_id,
             seq: event.seq,
             id: Proto.from_uuid(event.event_id),
             kind: Proto.kind(event.payload),
             actor_account_id: actor && actor.account_id,
             actor_device_id: actor && Map.get(actor, :device_id),
             command_id: command_id,
             acl_version: event.acl_version,
             created_at: now
           }),
         {:ok, _} <-
           Repo.insert(%EventPayload{channel_id: channel_id, seq: event.seq, payload: bytes}) do
      {:ok, {event, bytes}}
    end
  end

  defp insert_command(channel_id, command_id, actor, fingerprint, event, now) do
    %Command{
      channel_id: channel_id,
      command_id: command_id,
      actor_account_id: actor.account_id,
      fingerprint: fingerprint,
      seq: event.seq,
      event_id: Proto.from_uuid(event.event_id),
      created_at: now
    }
    |> Ecto.Changeset.change()
    |> Ecto.Changeset.unique_constraint(:command_id, name: :commands_pkey)
    |> Repo.insert()
  end

  # What a command names in the channel, checked and filled in before it is
  # written: a quote's original (its author, the text cut to its limit) and
  # a reaction's message and emoji (architecture 6.6). After the
  # fingerprint, so a retry of the same command still matches.
  defp resolve(
         channel_id,
         %V1.Command{
           op: {:send_message, %V1.SendMessage{reply_to: %V1.QuoteReply{} = quote} = op}
         } = command
       ) do
    case quotable(channel_id, quote.message_id) do
      {:ok, original} ->
        quote = %V1.QuoteReply{
          message_id: quote.message_id,
          author_account_id: Proto.uuid(original.author_account_id),
          text: String.slice(quote.text, 0, @quote_max),
          silent: quote.silent
        }

        {:ok, %{command | op: {:send_message, %{op | reply_to: quote}}}}

      error ->
        error
    end
  end

  defp resolve(channel_id, %V1.Command{op: {reaction, op}} = command)
       when reaction in [:add_reaction, :remove_reaction] do
    cond do
      not Lumia.Emoji.single?(op.reaction) -> {:error, :invalid_argument}
      match?({:ok, _}, quotable(channel_id, op.message_id)) -> {:ok, command}
      true -> {:error, :message_not_found}
    end
  end

  defp resolve(_channel_id, command), do: {:ok, command}

  # A message of this channel that is there to quote or react to: not
  # deleted, and not a notice.
  defp quotable(channel_id, message_id_bytes) do
    message_id = Proto.from_uuid(message_id_bytes)

    case message_id &&
           Repo.one(
             from m in Message,
               where:
                 m.id == ^message_id and m.channel_id == ^channel_id and is_nil(m.deleted_at) and
                   is_nil(m.notice)
           ) do
      %Message{} = message -> {:ok, message}
      _ -> {:error, :message_not_found}
    end
  end

  # Command op → event payload. The message id is the command id.
  defp payload_for(%V1.Command{op: {:send_message, op}} = command, actor, channel_id, seq, now) do
    message = %V1.Message{
      message_id: command.command_id,
      channel_id: Proto.uuid(channel_id),
      seq: seq,
      author_account_id: Proto.uuid(actor.account_id),
      parent_id: op.parent_id,
      body: op.body,
      mentions: op.mentions,
      created_at: Proto.timestamp(now),
      reply_to: op.reply_to
    }

    {:ok, {:message_created, %V1.MessageCreated{message: message}}}
  end

  defp payload_for(%V1.Command{op: {:edit_message, op}}, _actor, _channel_id, _seq, now) do
    edited = %V1.MessageEdited{
      message_id: op.message_id,
      body: op.body,
      mentions: op.mentions,
      edited_at: Proto.timestamp(now)
    }

    {:ok, {:message_edited, edited}}
  end

  defp payload_for(%V1.Command{op: {:delete_message, op}}, _actor, _channel_id, _seq, _now) do
    {:ok, {:message_deleted, %V1.MessageDeleted{message_id: op.message_id, purge_cache: true}}}
  end

  defp payload_for(%V1.Command{op: {:add_reaction, op}}, actor, _channel_id, _seq, _now) do
    added = %V1.ReactionAdded{
      message_id: op.message_id,
      reaction: op.reaction,
      account_id: Proto.uuid(actor.account_id)
    }

    {:ok, {:reaction_added, added}}
  end

  defp payload_for(%V1.Command{op: {:remove_reaction, op}}, actor, _channel_id, _seq, _now) do
    removed = %V1.ReactionRemoved{
      message_id: op.message_id,
      reaction: op.reaction,
      account_id: Proto.uuid(actor.account_id)
    }

    {:ok, {:reaction_removed, removed}}
  end

  defp payload_for(%V1.Command{op: {:pin_message, op}}, _, _, _, _),
    do: {:ok, {:message_pinned, %V1.MessagePinned{message_id: op.message_id}}}

  defp payload_for(%V1.Command{op: {:unpin_message, op}}, _, _, _, _),
    do: {:ok, {:message_unpinned, %V1.MessageUnpinned{message_id: op.message_id}}}

  defp payload_for(%V1.Command{op: {:send_encrypted, op}}, _, _, _, _),
    do: {:ok, {:encrypted, op.payload}}

  defp payload_for(%V1.Command{}, _, _, _, _), do: {:error, :invalid_command}

  # Materialized state. Only messages for now; reactions and pins follow
  # when their read side exists.
  defp materialize(channel_id, seq, actor, %V1.Command{op: {:send_message, op}} = command, now) do
    # A retry after the commands row was purged lands here: the message id is
    # the command id, so the insert collides and the caller sees a conflict,
    # never a duplicate message (docs/specs/delivery.md section 9).
    result =
      %Message{
        id: Proto.from_uuid(command.command_id),
        channel_id: channel_id,
        seq: seq,
        author_account_id: actor.account_id,
        parent_id: Proto.from_uuid(op.parent_id),
        body: op.body,
        created_at: now
      }
      |> with_quote(op.reply_to)
      |> with_mentions(op)
      |> Ecto.Changeset.change()
      |> Ecto.Changeset.unique_constraint(:id, name: :messages_pkey)
      |> Repo.insert()

    case result do
      {:error, %Ecto.Changeset{errors: [id: {_, [constraint: :unique] ++ _}]}} ->
        {:error, :conflict}

      other ->
        other
    end
  end

  defp materialize(channel_id, _seq, actor, %V1.Command{op: {:add_reaction, op}}, now) do
    %MessageReaction{
      message_id: Proto.from_uuid(op.message_id),
      account_id: actor.account_id,
      reaction: op.reaction,
      channel_id: channel_id,
      created_at: now
    }
    |> Repo.insert(on_conflict: :nothing)
  end

  defp materialize(_channel_id, _seq, actor, %V1.Command{op: {:remove_reaction, op}}, _now) do
    message_id = Proto.from_uuid(op.message_id)

    Repo.delete_all(
      from r in MessageReaction,
        where:
          r.message_id == ^message_id and r.account_id == ^actor.account_id and
            r.reaction == ^op.reaction
    )

    {:ok, nil}
  end

  defp materialize(channel_id, _seq, _actor, %V1.Command{op: {:edit_message, op}}, now) do
    # The edit's mentions replace the message's; the author a reply quotes
    # stays notified as it was (architecture 6.6).
    message_id = Proto.from_uuid(op.message_id)

    quoted =
      Repo.one(
        from m in Message,
          where:
            m.id == ^message_id and m.channel_id == ^channel_id and
              m.reply_to_author_id in m.mentioned_account_ids,
          select: m.reply_to_author_id
      )

    named = named_in(op.mentions)

    update_message(channel_id, op.message_id,
      set: [
        body: op.body,
        edited_at: now,
        mentioned_account_ids: Enum.uniq(named ++ List.wrap(quoted)),
        mentions_everyone: everyone_in?(op.mentions),
        mention_spans: Enum.flat_map(op.mentions, &Proto.mention_span/1)
      ]
    )
  end

  defp materialize(channel_id, _seq, _actor, %V1.Command{op: {:delete_message, op}}, now) do
    with {:ok, message_seq} <- update_message(channel_id, op.message_id, set: [deleted_at: now]) do
      # The original create event's payload is purged; the row stays.
      Repo.update_all(
        from(p in EventPayload, where: p.channel_id == ^channel_id and p.seq == ^message_seq),
        set: [payload: nil]
      )

      message_id = Proto.from_uuid(op.message_id)
      drop_quotes(channel_id, from(m in Message, where: m.id == ^message_id, select: m.id))
      {:ok, message_seq}
    end
  end

  defp materialize(_channel_id, _seq, _actor, _command, _now), do: {:ok, nil}

  defp with_quote(message, nil), do: message

  defp with_quote(message, %V1.QuoteReply{} = quote) do
    %{
      message
      | reply_to_id: Proto.from_uuid(quote.message_id),
        reply_to_author_id: Proto.from_uuid(quote.author_account_id),
        reply_text: quote.text
    }
  end

  # Whom the message notifies: the accounts it names and the author it
  # quotes, and whether it names everyone.
  defp with_mentions(message, %V1.SendMessage{} = op) do
    # The author quoted, unless the reply was sent silent.
    quoted =
      case op.reply_to do
        %V1.QuoteReply{silent: false} when message.reply_to_author_id != nil ->
          [message.reply_to_author_id]

        _ ->
          []
      end

    %{
      message
      | mentioned_account_ids: Enum.uniq(named_in(op.mentions) ++ quoted),
        mentions_everyone: everyone_in?(op.mentions),
        mention_spans: Enum.flat_map(op.mentions, &Proto.mention_span/1)
    }
  end

  defp named_in(mentions) do
    for %V1.Mention{target: {:account_id, id}} <- mentions,
        id = Proto.from_uuid(id),
        id != nil,
        do: id
  end

  defp everyone_in?(mentions),
    do: Enum.any?(mentions, &match?(%V1.Mention{target: {:everyone, true}}, &1))

  @doc """
  Drops the quoted text from the replies to the messages `quoted` (a query
  of message ids) in a channel, in their rows and in their stored events,
  which then say the original is gone: deleting a message still deletes
  it where it was quoted (architecture 6.6).
  """
  def drop_quotes(channel_id, quoted) do
    replies =
      Repo.all(
        from m in Message,
          where:
            m.channel_id == ^channel_id and m.reply_to_id in subquery(quoted) and
              not m.reply_deleted,
          select: m.seq
      )

    if replies != [] do
      Repo.update_all(
        from(m in Message, where: m.channel_id == ^channel_id and m.seq in ^replies),
        set: [reply_text: nil, reply_deleted: true]
      )

      from(p in EventPayload,
        where: p.channel_id == ^channel_id and p.seq in ^replies and not is_nil(p.payload)
      )
      |> Repo.all()
      |> Enum.each(&drop_quote_from/1)
    end

    :ok
  end

  defp drop_quote_from(%EventPayload{} = row) do
    case V1.Event.decode(row.payload) do
      %V1.Event{
        payload:
          {:message_created,
           %V1.MessageCreated{message: %V1.Message{reply_to: %V1.QuoteReply{} = quote} = message}}
      } = event ->
        quote = %{quote | text: "", original_deleted: true}

        event = %{
          event
          | payload: {:message_created, %V1.MessageCreated{message: %{message | reply_to: quote}}}
        }

        Repo.update_all(
          from(p in EventPayload, where: p.channel_id == ^row.channel_id and p.seq == ^row.seq),
          set: [payload: V1.Event.encode(event)]
        )

      _ ->
        :ok
    end
  end

  defp update_message(channel_id, message_id_bytes, updates) do
    message_id = Proto.from_uuid(message_id_bytes)

    query =
      from m in Message,
        where:
          m.id == ^message_id and m.channel_id == ^channel_id and is_nil(m.deleted_at) and
            is_nil(m.notice),
        select: m.seq

    case Repo.update_all(query, updates) do
      {1, [seq]} -> {:ok, seq}
      {0, _} -> {:error, :message_not_found}
    end
  end

  defp ack(%Command{} = c) do
    %{command_id: c.command_id, channel_id: c.channel_id, seq: c.seq, event_id: c.event_id}
  end

  # Deterministic over actor and the serialized command, so a retry matches
  # and a different command with a reused id does not.
  defp fingerprint(actor, %V1.Command{} = command) do
    :crypto.hash(:sha256, [actor.account_id, V1.Command.encode(command)])
  end

  defp redacted_kind("message_created"), do: :REDACTED_MESSAGE
  defp redacted_kind(_), do: :REDACTED_OTHER
end
