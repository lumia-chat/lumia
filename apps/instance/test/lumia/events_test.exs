defmodule Lumia.EventsTest do
  use Lumia.DataCase, async: true

  import Lumia.Fixtures

  alias Lumia.Events
  alias Lumia.Events.Command
  alias Lumia.Events.Event
  alias Lumia.Events.EventPayload
  alias Lumia.Events.Message
  alias Lumia.Proto
  alias Lumia.V1

  setup do
    %{channel: channel(), actor: actor()}
  end

  describe "append/4" do
    test "assigns dense, increasing seq per channel", %{channel: channel, actor: actor} do
      acks =
        for _ <- 1..3, do: elem(Events.append(channel.id, actor, send_message(channel.id)), 1)

      assert Enum.map(acks, & &1.seq) == [1, 2, 3]

      other = channel()
      assert {:ok, %{seq: 1}} = Events.append(other.id, actor, send_message(other.id))
    end

    test "writes event, serialized payload, command and message", %{
      channel: channel,
      actor: actor
    } do
      cmd = send_message(channel.id, "hello")
      command_id = Proto.from_uuid(cmd.command_id)
      assert {:ok, ack} = Events.append(channel.id, actor, cmd)

      assert %Event{kind: "message_created", command_id: ^command_id, acl_version: 1} =
               Repo.get_by!(Event, channel_id: channel.id, seq: ack.seq)

      %EventPayload{payload: bytes} = Repo.get_by!(EventPayload, channel_id: channel.id, seq: 1)

      assert %V1.Event{seq: 1, payload: {:message_created, %V1.MessageCreated{message: message}}} =
               V1.Event.decode(bytes)

      assert message.body == "hello"
      assert message.message_id == cmd.command_id

      assert %Command{seq: 1} =
               Repo.get_by!(Command, channel_id: channel.id, command_id: command_id)

      assert %Message{body: "hello", seq: 1} = Repo.get!(Message, command_id)
    end

    test "broadcasts the same bytes it stored, after commit", %{channel: channel, actor: actor} do
      Phoenix.PubSub.subscribe(Lumia.PubSub, Events.topic(channel.id))
      assert {:ok, _} = Events.append(channel.id, actor, send_message(channel.id))
      assert_receive {:event, %V1.Event{seq: 1} = event, bytes}
      assert V1.Event.encode(event) == bytes

      assert %EventPayload{payload: ^bytes} =
               Repo.get_by!(EventPayload, channel_id: channel.id, seq: 1)
    end

    test "advances the channel head", %{channel: channel, actor: actor} do
      assert {:ok, _} = Events.append(channel.id, actor, send_message(channel.id))
      channel = Repo.reload!(channel)
      assert channel.last_seq == 1
      assert %DateTime{microsecond: {0, 0}} = channel.last_seq_at
    end

    test "replaying an identical command returns the original ack and writes nothing", ctx do
      cmd = send_message(ctx.channel.id)
      assert {:ok, ack} = Events.append(ctx.channel.id, ctx.actor, cmd)
      assert {:ok, ^ack} = Events.append(ctx.channel.id, ctx.actor, cmd)
      assert Repo.aggregate(Event, :count) == 1
      assert Repo.reload!(ctx.channel).last_seq == 1
    end

    test "reusing a command id with a different operation is a conflict", ctx do
      cmd = send_message(ctx.channel.id, "one")
      assert {:ok, _} = Events.append(ctx.channel.id, ctx.actor, cmd)
      changed = %{cmd | op: {:send_message, %V1.SendMessage{body: "two"}}}
      assert {:error, :conflict} = Events.append(ctx.channel.id, ctx.actor, changed)
      assert Repo.aggregate(Event, :count) == 1
    end

    test "the same command from a different actor is a conflict", ctx do
      cmd = send_message(ctx.channel.id)
      assert {:ok, _} = Events.append(ctx.channel.id, ctx.actor, cmd)
      assert {:error, :conflict} = Events.append(ctx.channel.id, actor(), cmd)
    end

    test "edits update the message and append an event", ctx do
      cmd = send_message(ctx.channel.id, "draft")
      {:ok, _} = Events.append(ctx.channel.id, ctx.actor, cmd)
      message_id = Proto.from_uuid(cmd.command_id)

      assert {:ok, %{seq: 2}} =
               Events.append(
                 ctx.channel.id,
                 ctx.actor,
                 edit_message(ctx.channel.id, message_id, "final")
               )

      assert %Message{body: "final", edited_at: %DateTime{}} = Repo.get!(Message, message_id)
    end

    test "deletes tombstone the message and redact the create event", ctx do
      cmd = send_message(ctx.channel.id, "secret")
      {:ok, _} = Events.append(ctx.channel.id, ctx.actor, cmd)
      message_id = Proto.from_uuid(cmd.command_id)

      assert {:ok, %{seq: 2}} =
               Events.append(
                 ctx.channel.id,
                 ctx.actor,
                 delete_message(ctx.channel.id, message_id)
               )

      assert %Message{deleted_at: %DateTime{}} = Repo.get!(Message, message_id)

      assert [
               %V1.Event{seq: 1, payload: {:redacted, %V1.Redacted{was: :REDACTED_MESSAGE}}},
               %V1.Event{seq: 2, payload: {:message_deleted, _}}
             ] =
               Events.events_after(ctx.channel.id, 0)
    end

    test "unknown channel", %{actor: actor} do
      id = Ecto.UUID.generate()
      assert {:error, :channel_not_found} = Events.append(id, actor, send_message(id))
    end
  end

  describe "append_system/3" do
    test "appends without a command and broadcasts", %{channel: channel} do
      Phoenix.PubSub.subscribe(Lumia.PubSub, Events.topic(channel.id))

      assert {:ok, %{seq: 1}} =
               Events.append_system(channel.id, {:acl_changed, %V1.AclChanged{acl_version: 1}})

      assert_receive {:event, %V1.Event{seq: 1, payload: {:acl_changed, _}}, _bytes}
      assert Repo.aggregate(Command, :count) == 0
    end
  end

  describe "events_after/3" do
    test "returns events strictly after the cursor, in order, decoded", %{
      channel: channel,
      actor: actor
    } do
      for i <- 1..5,
          do: {:ok, _} = Events.append(channel.id, actor, send_message(channel.id, "m#{i}"))

      events = Events.events_after(channel.id, 2)
      assert Enum.map(events, & &1.seq) == [3, 4, 5]

      assert [
               %V1.Event{
                 payload: {:message_created, %V1.MessageCreated{message: %V1.Message{body: "m3"}}}
               }
               | _
             ] = events

      assert Events.events_after(channel.id, 5) == []
    end
  end

  describe "changed_since/1" do
    test "lists channels whose head moved", %{channel: channel, actor: actor} do
      since = DateTime.add(DateTime.utc_now(), -1, :second)
      quiet = channel()
      {:ok, _} = Events.append(channel.id, actor, send_message(channel.id))

      changed = Events.changed_since(since)
      assert {channel.id, 1} in changed
      refute Enum.any?(changed, fn {id, _} -> id == quiet.id end)
    end
  end

  describe "schema" do
    test "event-log tables are hash-partitioned 32 ways" do
      for table <- ~w(events event_payloads commands) do
        %{rows: [[count]]} =
          Repo.query!(
            "SELECT count(*) FROM pg_inherits WHERE inhparent = to_regclass($1::text)",
            [table]
          )

        assert count == 32, "#{table} has #{count} partitions"
      end
    end
  end
end
