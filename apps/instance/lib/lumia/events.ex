defmodule Lumia.Events do
  @moduledoc """
  The event log: append commands to channels, assign `seq`, keep acks for
  idempotent replay.

  This is the write path from `docs/architecture.md` sections 8.1 to 8.3.
  One transaction per command: lock the channel row by incrementing
  `last_seq`, insert the event, its payload and the command record, and
  materialize the message. Broadcast is the caller's job and happens only
  after this function returns `{:ok, ack}`.
  """

  import Ecto.Query

  alias Lumia.Events.Channel
  alias Lumia.Events.Command
  alias Lumia.Events.Event
  alias Lumia.Events.EventPayload
  alias Lumia.Events.Message
  alias Lumia.Repo
  alias Lumia.Schema

  @type command :: %{
          required(:command_id) => Ecto.UUID.t(),
          required(:kind) => String.t(),
          required(:payload) => map(),
          optional(:actor_account_id) => Ecto.UUID.t() | nil,
          optional(:actor_device_id) => Ecto.UUID.t() | nil
        }

  @type ack :: %{
          command_id: Ecto.UUID.t(),
          channel_id: Ecto.UUID.t(),
          seq: pos_integer(),
          event_id: Ecto.UUID.t()
        }

  @doc "Creates a channel. Spike-level; the real context lives elsewhere later."
  @spec create_channel(String.t()) :: {:ok, Channel.t()} | {:error, Ecto.Changeset.t()}
  def create_channel(kind \\ "text") do
    Repo.insert(%Channel{kind: kind})
  end

  @doc """
  Appends a command to a channel log.

  Returns the ack for the committed event. A replay of an already accepted
  command (same id, same fingerprint) returns the original ack without
  writing. The same id with a different operation is `{:error, :conflict}`.
  """
  @spec append(Ecto.UUID.t(), command()) ::
          {:ok, ack()} | {:error, :conflict | :channel_not_found | Ecto.Changeset.t()}
  def append(channel_id, %{command_id: command_id} = command) do
    fingerprint = fingerprint(command)

    result =
      Repo.transact(fn ->
        case Repo.get_by(Command, channel_id: channel_id, command_id: command_id) do
          %Command{fingerprint: ^fingerprint} = existing -> {:ok, ack(existing)}
          %Command{} -> {:error, :conflict}
          nil -> write(channel_id, command, fingerprint)
        end
      end)

    case result do
      # A concurrent identical command won the race on the unique key. The
      # whole transaction rolled back, so the seq it took is gone too.
      {:error, %Ecto.Changeset{errors: [command_id: {_, [constraint: :unique] ++ _}]}} ->
        append(channel_id, command)

      other ->
        other
    end
  end

  defp write(channel_id, command, fingerprint) do
    now = DateTime.utc_now()

    # Locks the row for the rest of the transaction and assigns seq in one
    # statement. Everything after this is serialized per channel.
    lock =
      from(c in Channel, where: c.id == ^channel_id, select: {c.last_seq, c.acl_version})

    # Truncated to the second so that writes within the same second do not
    # change an indexed column and the row update stays HOT.
    head_at = DateTime.truncate(now, :second)

    case Repo.update_all(lock, inc: [last_seq: 1], set: [last_seq_at: head_at]) do
      {0, _} ->
        {:error, :channel_not_found}

      {1, [{seq, acl_version}]} ->
        event_id = Schema.generate_id()

        with {:ok, _} <-
               Repo.insert(%Event{
                 channel_id: channel_id,
                 seq: seq,
                 id: event_id,
                 kind: command.kind,
                 actor_account_id: Map.get(command, :actor_account_id),
                 actor_device_id: Map.get(command, :actor_device_id),
                 command_id: command.command_id,
                 acl_version: acl_version,
                 created_at: now
               }),
             {:ok, _} <-
               Repo.insert(%EventPayload{
                 channel_id: channel_id,
                 seq: seq,
                 payload: command.payload
               }),
             {:ok, _} <- materialize(channel_id, seq, command, now),
             {:ok, record} <-
               %Command{
                 channel_id: channel_id,
                 command_id: command.command_id,
                 actor_account_id: Map.get(command, :actor_account_id),
                 fingerprint: fingerprint,
                 seq: seq,
                 event_id: event_id,
                 created_at: now
               }
               |> Ecto.Changeset.change()
               |> Ecto.Changeset.unique_constraint(:command_id, name: :commands_pkey)
               |> Repo.insert() do
          {:ok, ack(record)}
        end
    end
  end

  # Only message creation is materialized in the spike. Edits, deletes and
  # reactions follow when the full event types land.
  defp materialize(channel_id, seq, %{kind: "message_created"} = command, now) do
    Repo.insert(%Message{
      id: command.command_id,
      channel_id: channel_id,
      seq: seq,
      author_account_id: Map.get(command, :actor_account_id),
      parent_id: command.payload["parent_id"],
      body: command.payload["body"],
      created_at: now
    })
  end

  defp materialize(_channel_id, _seq, _command, _now), do: {:ok, nil}

  @doc "Events after `after_seq`, in order. The sync query."
  @spec events_after(Ecto.UUID.t(), non_neg_integer(), pos_integer()) :: [Event.t()]
  def events_after(channel_id, after_seq, limit \\ 200) do
    Repo.all(
      from e in Event,
        where: e.channel_id == ^channel_id and e.seq > ^after_seq,
        order_by: [asc: e.seq],
        limit: ^limit
    )
  end

  @doc """
  Channels whose head moved at or after `since`. The reconciliation query.

  Inclusive, and `last_seq_at` has second granularity, so a channel written
  in the same second as `since` is returned again. Spurious nudges are
  harmless (architecture 8.2); missed ones are not.
  """
  @spec changed_since(DateTime.t()) :: [{Ecto.UUID.t(), pos_integer()}]
  def changed_since(%DateTime{} = since) do
    since = DateTime.truncate(since, :second)

    Repo.all(
      from c in Channel,
        where: c.last_seq_at >= ^since,
        select: {c.id, c.last_seq}
    )
  end

  defp ack(%Command{} = c) do
    %{command_id: c.command_id, channel_id: c.channel_id, seq: c.seq, event_id: c.event_id}
  end

  # Deterministic over the operation, so a retry matches and a different
  # command with a reused id does not.
  defp fingerprint(command) do
    term = {command.kind, Map.get(command, :actor_account_id), command.payload}
    :crypto.hash(:sha256, :erlang.term_to_binary(term, [:deterministic]))
  end
end
