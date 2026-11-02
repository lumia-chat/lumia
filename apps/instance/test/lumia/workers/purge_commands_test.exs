defmodule Lumia.Workers.PurgeCommandsTest do
  use Lumia.DataCase, async: true
  use Oban.Testing, repo: Lumia.Repo

  import Lumia.Fixtures

  alias Lumia.Events
  alias Lumia.Events.Command
  alias Lumia.Workers.PurgeCommands

  test "deletes commands older than the retention window and keeps the rest" do
    channel = channel()
    actor = actor()
    {:ok, old} = Events.append(channel.id, actor, send_message(channel.id))
    {:ok, fresh} = Events.append(channel.id, actor, send_message(channel.id))

    stale = DateTime.add(PurgeCommands.cutoff(), -1, :day)

    Repo.update_all(
      from(c in Command, where: c.channel_id == ^channel.id and c.command_id == ^old.command_id),
      set: [created_at: stale]
    )

    assert {:ok, 1} = perform_job(PurgeCommands, %{})

    refute Repo.get_by(Command, channel_id: channel.id, command_id: old.command_id)
    assert Repo.get_by(Command, channel_id: channel.id, command_id: fresh.command_id)
  end

  test "a purged message command cannot create a duplicate message on retry" do
    channel = channel()
    actor = actor()
    cmd = send_message(channel.id)
    {:ok, ack} = Events.append(channel.id, actor, cmd)

    Repo.delete_all(
      from(c in Command, where: c.channel_id == ^channel.id and c.command_id == ^ack.command_id)
    )

    assert {:error, :conflict} = Events.append(channel.id, actor, cmd)
    assert Repo.aggregate(Lumia.Events.Message, :count) == 1
  end
end
