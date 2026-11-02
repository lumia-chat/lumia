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

  alias Lumia.Events.Channel
  alias Lumia.Events.Command
  alias Lumia.Events.Event
  alias Lumia.Events.EventPayload
  alias Lumia.Events.Message
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.Schema
  alias Lumia.V1

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
    command_id = Proto.from_uuid(command.command_id)
    fingerprint = fingerprint(actor, command)

    result =
      Repo.transact(fn ->
        case Repo.get_by(Command, channel_id: channel_id, command_id: command_id) do
          %Command{fingerprint: ^fingerprint} = existing -> {:ok, {ack(existing), nil}}
          %Command{} -> {:error, :conflict}
          nil -> write(channel_id, actor, command, command_id, fingerprint)
        end
      end)

    case result do
      {:ok, {ack, nil}} ->
        {:ok, ack}

      {:ok, {ack, {event, bytes}}} ->
        if Keyword.get(opts, :broadcast, true), do: broadcast(channel_id, event, bytes)
        {:ok, ack}

      # A concurrent identical command won the race on the unique key. The
      # whole transaction rolled back, so the seq it took is gone too.
      {:error, %Ecto.Changeset{errors: [command_id: {_, [constraint: :unique] ++ _}]}} ->
        append(channel_id, actor, command, opts)

      other ->
        other
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
      Repo.transact(fn ->
        now = DateTime.utc_now()

        with {:ok, seq, acl_version} <- take_head(channel_id, now) do
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
      if Keyword.get(opts, :broadcast, true), do: broadcast(channel_id, event, bytes)
      {:ok, %{channel_id: channel_id, seq: event.seq, event_id: Proto.from_uuid(event.event_id)}}
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

    with {:ok, seq, acl_version} <- take_head(channel_id, now),
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
    query = from(c in Channel, where: c.id == ^channel_id, select: {c.last_seq, c.acl_version})

    case Repo.update_all(query, inc: [last_seq: 1], set: [last_seq_at: head_at]) do
      {0, _} -> {:error, :channel_not_found}
      {1, [{seq, acl_version}]} -> {:ok, seq, acl_version}
    end
  end

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
      created_at: Proto.timestamp(now)
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

  defp materialize(channel_id, _seq, _actor, %V1.Command{op: {:edit_message, op}}, now) do
    update_message(channel_id, op.message_id, set: [body: op.body, edited_at: now])
  end

  defp materialize(channel_id, _seq, _actor, %V1.Command{op: {:delete_message, op}}, now) do
    with {:ok, message_seq} <- update_message(channel_id, op.message_id, set: [deleted_at: now]) do
      # The original create event's payload is purged; the row stays.
      Repo.update_all(
        from(p in EventPayload, where: p.channel_id == ^channel_id and p.seq == ^message_seq),
        set: [payload: nil]
      )

      {:ok, message_seq}
    end
  end

  defp materialize(_channel_id, _seq, _actor, _command, _now), do: {:ok, nil}

  defp update_message(channel_id, message_id_bytes, updates) do
    message_id = Proto.from_uuid(message_id_bytes)

    query =
      from m in Message,
        where: m.id == ^message_id and m.channel_id == ^channel_id and is_nil(m.deleted_at),
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
