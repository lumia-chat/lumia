defmodule Lumia.EventsTest do
  use Lumia.DataCase, async: true

  alias Lumia.Events
  alias Lumia.Events.Command
  alias Lumia.Events.Event
  alias Lumia.Events.EventPayload
  alias Lumia.Events.Message

  setup do
    {:ok, channel} = Events.create_channel()
    %{channel: channel}
  end

  defp command(overrides \\ %{}) do
    Map.merge(
      %{
        command_id: Ecto.UUID.generate(version: 7),
        kind: "message_created",
        actor_account_id: Ecto.UUID.generate(),
        payload: %{"body" => "hello"}
      },
      overrides
    )
  end

  describe "append/2" do
    test "assigns dense, increasing seq per channel", %{channel: channel} do
      acks = for _ <- 1..3, do: elem(Events.append(channel.id, command()), 1)
      assert Enum.map(acks, & &1.seq) == [1, 2, 3]

      {:ok, other} = Events.create_channel()
      assert {:ok, %{seq: 1}} = Events.append(other.id, command())
    end

    test "writes event, payload, command and message in one go", %{channel: channel} do
      cmd = command()
      assert {:ok, ack} = Events.append(channel.id, cmd)

      assert %Event{kind: "message_created", command_id: command_id, acl_version: 1} =
               Repo.get_by!(Event, channel_id: channel.id, seq: ack.seq)

      assert command_id == cmd.command_id

      assert %EventPayload{payload: %{"body" => "hello"}} =
               Repo.get_by!(EventPayload, channel_id: channel.id, seq: 1)

      assert %Command{seq: 1} =
               Repo.get_by!(Command, channel_id: channel.id, command_id: cmd.command_id)

      assert %Message{body: "hello", seq: 1} = Repo.get!(Message, cmd.command_id)
    end

    test "advances the channel head", %{channel: channel} do
      assert {:ok, _} = Events.append(channel.id, command())
      channel = Repo.reload!(channel)
      assert channel.last_seq == 1
      assert %DateTime{} = channel.last_seq_at
    end

    test "replaying an identical command returns the original ack and writes nothing", %{
      channel: channel
    } do
      cmd = command()
      assert {:ok, ack} = Events.append(channel.id, cmd)
      assert {:ok, ^ack} = Events.append(channel.id, cmd)
      assert Repo.aggregate(Event, :count) == 1
      assert Repo.reload!(channel).last_seq == 1
    end

    test "reusing a command id with a different operation is a conflict", %{channel: channel} do
      cmd = command()
      assert {:ok, _} = Events.append(channel.id, cmd)

      assert {:error, :conflict} =
               Events.append(channel.id, %{cmd | payload: %{"body" => "changed"}})

      assert Repo.aggregate(Event, :count) == 1
    end

    test "unknown channel", %{} do
      assert {:error, :channel_not_found} = Events.append(Ecto.UUID.generate(), command())
    end
  end

  describe "events_after/3" do
    test "returns events strictly after the cursor, in order", %{channel: channel} do
      for _ <- 1..5, do: {:ok, _} = Events.append(channel.id, command())
      assert Enum.map(Events.events_after(channel.id, 2), & &1.seq) == [3, 4, 5]
      assert Events.events_after(channel.id, 5) == []
    end
  end

  describe "changed_since/1" do
    test "lists channels whose head moved", %{channel: channel} do
      since = DateTime.add(DateTime.utc_now(), -1, :second)
      {:ok, quiet} = Events.create_channel()
      {:ok, _} = Events.append(channel.id, command())

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
