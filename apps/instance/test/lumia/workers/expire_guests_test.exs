defmodule Lumia.Workers.ExpireGuestsTest do
  use Lumia.DataCase, async: true

  import Lumia.Fixtures

  alias Lumia.Accounts
  alias Lumia.Servers
  alias Lumia.Workers.ExpireGuests

  setup do
    owner = account("owner")
    {:ok, server} = Servers.create_server(owner.id, "Acme")
    {:ok, channel} = Servers.create_channel(server.id, owner.id, "general")
    Lumia.Workers.ViewerDiff.diff_channel(channel.id)
    %{owner: owner, server: server, channel: channel}
  end

  test "removes expired guests with the guest_expired reason and leaves the rest", ctx do
    now = DateTime.utc_now()
    expired = account("expired")
    current = account("current")
    member = account("member")

    {:ok, _} =
      Servers.add_member(ctx.server.id, expired.id, ctx.owner.id,
        kind: "guest",
        expires_at: DateTime.add(now, -60, :second)
      )

    {:ok, _} =
      Servers.add_member(ctx.server.id, current.id, ctx.owner.id,
        kind: "guest",
        expires_at: DateTime.add(now, 3600, :second)
      )

    {:ok, _} = Servers.add_member(ctx.server.id, member.id, ctx.owner.id)
    Lumia.Workers.ViewerDiff.diff_channel(ctx.channel.id)

    assert {:ok, 1} = perform_job(ExpireGuests, %{})

    refute Servers.member_for(ctx.server.id, expired.id)
    assert Servers.member_for(ctx.server.id, current.id)
    assert Servers.member_for(ctx.server.id, member.id)

    # A guest holds nothing from @everyone, so there was no viewable channel
    # to remove: the membership event alone tells the client to drop the server.
    kinds = Accounts.event_kinds(expired.id)
    assert "membership_removed" in kinds
    refute "channel_removed" in kinds
    refute "membership_removed" in Accounts.event_kinds(current.id)

    # Idempotent: nothing left to expire.
    assert {:ok, 0} = perform_job(ExpireGuests, %{})
  end

  defp perform_job(worker, args) do
    worker.perform(%Oban.Job{args: args})
  end
end
