defmodule LumiaWeb.Gateway.MultiNodeTest do
  @moduledoc """
  The cross-node cases of docs/specs/deployment.md section 8 (delivery
  section 12), on two nodes: this one, holding the sessions, and a peer
  started by `Lumia.PeerNode` on the same database. Not async and not in
  the sandbox: the peer's writes are real, so the module truncates the
  tables when it is done.
  """

  use ExUnit.Case, async: false

  import Lumia.Fixtures
  import Phoenix.ChannelTest

  alias Ecto.Adapters.SQL.Sandbox
  alias Lumia.Auth
  alias Lumia.Authz.Access
  alias Lumia.Events
  alias Lumia.PeerNode
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.Servers
  alias Lumia.V1
  alias LumiaWeb.Gateway.Reconciler
  alias LumiaWeb.Gateway.SessionChannel
  alias LumiaWeb.Gateway.Socket

  @endpoint LumiaWeb.Endpoint

  setup_all do
    Sandbox.mode(Repo, :auto)
    {pid, node} = PeerNode.start(:lumia_peer)

    on_exit(fn ->
      PeerNode.stop(pid)
      PeerNode.truncate_all()
      Sandbox.mode(Repo, :manual)
    end)

    %{peer: pid, node: node}
  end

  setup do
    owner = account("owner")
    device_id = Ecto.UUID.generate()
    {:ok, _session, tokens} = Auth.create_session(owner.id, device_id)
    {:ok, socket} = connect(Socket, %{}, connect_info: %{auth_token: tokens.access_token})
    {:ok, server} = Servers.create_server(owner.id, "Nodes")
    {:ok, channel} = Servers.create_channel(server.id, owner.id, "general")
    hello = %V1.Hello{client: "test", device_id: Proto.uuid(device_id)}
    {:ok, %V1.Welcome{}, socket} = subscribe_and_join(socket, SessionChannel, "session", hello)

    push(socket, "frame", %V1.ClientFrame{
      frame:
        {:subscribe, %V1.Subscribe{cursors: [%V1.Cursor{log_id: Proto.uuid(channel.id), seq: 0}]}}
    })

    assert_push "frame", %V1.ServerFrame{
      frame: {:subscribe_result, %V1.SubscribeResult{results: [%{status: :RESUMED}]}}
    }

    %{owner: owner, device_id: device_id, server: server, channel: channel, socket: socket}
  end

  defp who(ctx), do: %{account_id: ctx.owner.id, device_id: ctx.device_id}

  defp append_on_peer(ctx, body, opts) do
    {:ok, ack} =
      PeerNode.call(ctx.peer, Events, :append, [
        ctx.channel.id,
        who(ctx),
        send_message(ctx.channel.id, body),
        opts
      ])

    ack
  end

  defp tick, do: Reconciler.tick(DateTime.add(DateTime.utc_now(), -10, :second))

  test "a commit on the other node that never broadcast reaches a session here within one tick",
       ctx do
    append_on_peer(ctx, "from B, unannounced", broadcast: false)
    refute_push "frame", {:event_bytes, _}

    assert tick() >= 1
    assert_push "frame", {:event_bytes, bytes}
    assert V1.Event.decode(bytes).seq == 1
    refute_push "frame", {:event_bytes, _}
  end

  test "a partition heals through reconciliation, delivering once", ctx do
    true = Node.disconnect(ctx.node)
    append_on_peer(ctx, "from B, during the partition", [])
    refute_push "frame", {:event_bytes, _}

    true = Node.connect(ctx.node)
    assert tick() >= 1
    assert_push "frame", {:event_bytes, bytes}
    assert V1.Event.decode(bytes).seq == 1
    refute_push "frame", {:event_bytes, _}
  end

  test "a removal here reaches the other node: no resolution for the member, the removal on its log",
       ctx do
    member = account("member")
    {:ok, _} = Servers.add_member(ctx.server.id, member.id, ctx.owner.id)
    # The viewer set is kept by a worker; run its pass here, as the single-node test does.
    Lumia.Workers.ViewerDiff.diff_channel(ctx.channel.id)

    assert {:ok, %{acl_version: _}} =
             PeerNode.call(ctx.peer, Access, :channel, [member.id, ctx.channel.id])

    :ok = Servers.remove_member(ctx.server.id, member.id, ctx.owner.id, "removed")
    {:ok, _} = Events.append(ctx.channel.id, who(ctx), send_message(ctx.channel.id, "after"))

    # Member-level changes travel on the account log, not the acl version
    # (authorization spec): the peer resolves against the database and
    # finds no membership, and the member's log carries the removal.
    assert {:error, :not_found} =
             PeerNode.call(ctx.peer, Access, :channel, [member.id, ctx.channel.id])

    kinds = PeerNode.call(ctx.peer, Lumia.Accounts, :event_kinds, [member.id])
    assert "membership_removed" in kinds and "channel_removed" in kinds
  end

  test "broadcasts from the other node out of order are delivered in order, once", ctx do
    append_on_peer(ctx, "one", broadcast: false)
    append_on_peer(ctx, "two", broadcast: false)
    [one, two] = PeerNode.call(ctx.peer, Events, :events_after, [ctx.channel.id, 0, 10])

    :ok = PeerNode.call(ctx.peer, Events, :broadcast, [ctx.channel.id, two, V1.Event.encode(two)])
    :ok = PeerNode.call(ctx.peer, Events, :broadcast, [ctx.channel.id, one, V1.Event.encode(one)])

    assert_push "frame", {:event_bytes, first}
    assert_push "frame", {:event_bytes, second}
    assert V1.Event.decode(first).seq == 1
    assert V1.Event.decode(second).seq == 2
    refute_push "frame", {:event_bytes, _}
  end
end
