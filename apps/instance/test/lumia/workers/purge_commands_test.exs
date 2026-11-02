defmodule Lumia.Workers.PurgeCommandsTest do
  use Lumia.DataCase, async: true
  use Oban.Testing, repo: Lumia.Repo

  alias Lumia.Events
  alias Lumia.Events.Command
  alias Lumia.Workers.PurgeCommands

  defp command do
    %{
      command_id: Ecto.UUID.generate(version: 7),
      kind: "message_created",
      actor_account_id: Ecto.UUID.generate(),
      payload: %{"body" => "hi"}
    }
  end

  test "deletes commands older than the retention window and keeps the rest" do
    {:ok, channel} = Events.create_channel()
    {:ok, old} = Events.append(channel.id, command())
    {:ok, fresh} = Events.append(channel.id, command())

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
    {:ok, channel} = Events.create_channel()
    cmd = command()
    {:ok, ack} = Events.append(channel.id, cmd)

    Repo.delete_all(
      from(c in Command, where: c.channel_id == ^channel.id and c.command_id == ^ack.command_id)
    )

    assert {:error, :conflict} = Events.append(channel.id, cmd)
    assert Repo.aggregate(Lumia.Events.Message, :count) == 1
  end
end
