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
  import Lumia.IdentityFixtures
  import Phoenix.ChannelTest

  alias Ecto.Adapters.SQL.Sandbox
  alias Lumia.Auth
  alias Lumia.Authz.Access
  alias Lumia.Crypto
  alias Lumia.Events
  alias Lumia.Identity
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

  # The owner's socket leaves, so only the member's pushes reach the test process.
  defp close_owner(ctx) do
    Process.unlink(ctx.socket.channel_pid)
    ref = leave(ctx.socket)
    assert_reply ref, :ok
  end

  # A member with a session here, subscribed to the channel from the start.
  defp member_socket(ctx) do
    member = account("member")
    {:ok, _} = Servers.add_member(ctx.server.id, member.id, ctx.owner.id)
    Lumia.Workers.ViewerDiff.diff_channel(ctx.channel.id)
    device_id = Ecto.UUID.generate()
    {:ok, _, tokens} = Auth.create_session(member.id, device_id)
    {:ok, socket} = connect(Socket, %{}, connect_info: %{auth_token: tokens.access_token})
    hello = %V1.Hello{client: "test", device_id: Proto.uuid(device_id)}
    {:ok, %V1.Welcome{}, socket} = subscribe_and_join(socket, SessionChannel, "session", hello)

    push(socket, "frame", %V1.ClientFrame{
      frame:
        {:subscribe,
         %V1.Subscribe{cursors: [%V1.Cursor{log_id: Proto.uuid(ctx.channel.id), seq: 0}]}}
    })

    assert_push "frame", %V1.ServerFrame{
      frame: {:subscribe_result, %V1.SubscribeResult{results: [%{status: :RESUMED}]}}
    }

    {member, socket}
  end

  defp subscriptions(socket), do: :sys.get_state(socket.channel_pid).assigns.subscriptions

  defp proof(identity, device, roster, purpose) do
    {challenge, _} = Identity.issue_challenge(purpose)

    %V1.IdentityProof{
      challenge: challenge,
      identity_id: identity.identity_id,
      device_id: device.id,
      signature: %V1.Signature{
        signer_kind: :DEVICE,
        signer_id: device.id,
        signature: Crypto.sign(challenge, device.priv)
      },
      roster: roster
    }
  end

  @tag failure_injection: [delivery: 1]
  test "a commit on the other node that never broadcast reaches a session here within one tick",
       ctx do
    append_on_peer(ctx, "from B, unannounced", broadcast: false)
    refute_push "frame", {:event_bytes, _}

    assert tick() >= 1
    assert_push "frame", {:event_bytes, bytes}, 5_000
    assert V1.Event.decode(bytes).seq == 1
    refute_push "frame", {:event_bytes, _}
  end

  @tag failure_injection: [delivery: 4]
  test "a partition heals through reconciliation, delivering once", ctx do
    true = Node.disconnect(ctx.node)
    append_on_peer(ctx, "from B, during the partition", [])
    refute_push "frame", {:event_bytes, _}

    true = Node.connect(ctx.node)
    assert tick() >= 1
    assert_push "frame", {:event_bytes, bytes}, 5_000
    assert V1.Event.decode(bytes).seq == 1
    refute_push "frame", {:event_bytes, _}
  end

  @tag failure_injection: [delivery: 3]
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

  @tag failure_injection: [delivery: 2]
  test "broadcasts from the other node out of order are delivered in order, once", ctx do
    append_on_peer(ctx, "one", broadcast: false)
    append_on_peer(ctx, "two", broadcast: false)
    [one, two] = PeerNode.call(ctx.peer, Events, :events_after, [ctx.channel.id, 0, 10])

    :ok = PeerNode.call(ctx.peer, Events, :broadcast, [ctx.channel.id, two, V1.Event.encode(two)])
    :ok = PeerNode.call(ctx.peer, Events, :broadcast, [ctx.channel.id, one, V1.Event.encode(one)])

    assert_push "frame", {:event_bytes, first}, 5_000
    assert_push "frame", {:event_bytes, second}, 5_000
    assert V1.Event.decode(first).seq == 1
    assert V1.Event.decode(second).seq == 2
    refute_push "frame", {:event_bytes, _}
  end

  @tag failure_injection: [delivery: 3, authorization: 3]
  test "a kick on the other node ends the member's subscription here, before anything committed after it",
       ctx do
    close_owner(ctx)
    {member, socket} = member_socket(ctx)

    append_on_peer(ctx, "before", [])
    assert_push "frame", {:event_bytes, bytes}, 5_000
    assert V1.Event.decode(bytes).seq == 1

    :ok =
      PeerNode.call(ctx.peer, Servers, :remove_member, [
        ctx.server.id,
        member.id,
        ctx.owner.id,
        "removed"
      ])

    append_on_peer(ctx, "after", [])
    tick()
    refute_push "frame", {:event_bytes, _}, 500
    assert subscriptions(socket) == %{}
  end

  @tag failure_injection: [authorization: 4, delivery: 4]
  test "overwrites changed on the other node during a partition are applied here before anything is delivered once it heals",
       ctx do
    close_owner(ctx)
    {_member, socket} = member_socket(ctx)
    everyone = Repo.get_by!(Lumia.Servers.Role, server_id: ctx.server.id, is_everyone: true)

    true = Node.disconnect(ctx.node)

    :ok =
      PeerNode.call(ctx.peer, Servers, :set_overwrite, [
        ctx.channel.id,
        ctx.owner.id,
        {:role, everyone.id},
        [],
        [:view_channel]
      ])

    append_on_peer(ctx, "private now", [])
    refute_push "frame", {:event_bytes, _}

    true = Node.connect(ctx.node)
    assert tick() >= 1
    refute_push "frame", {:event_bytes, _}, 500
    assert subscriptions(socket) == %{}
  end

  @tag failure_injection: [delivery: 9]
  test "the same command sent to both nodes at once commits once, and both callers get the same ack",
       ctx do
    for i <- 1..5 do
      command = send_message(ctx.channel.id, "once #{i}")
      here = Task.async(fn -> Events.append(ctx.channel.id, who(ctx), command) end)

      there =
        Task.async(fn ->
          PeerNode.call(ctx.peer, Events, :append, [ctx.channel.id, who(ctx), command, []])
        end)

      assert [{:ok, ack_here}, {:ok, ack_there}] = Task.await_many([here, there], 10_000)
      assert ack_here == ack_there
      assert {^i, _} = Events.head(ctx.channel.id)
      assert_push "frame", {:event_bytes, bytes}, 5_000
      assert V1.Event.decode(bytes).seq == i
    end

    assert Enum.map(Events.events_after(ctx.channel.id, 0, 10), & &1.seq) == [1, 2, 3, 4, 5]
    refute_push "frame", {:event_bytes, _}, 500
  end

  @tag failure_injection: [delivery: 11]
  test "a snapshot taken while the other node writes holds the messages up to its seq, and the rest are fetched once after it",
       ctx do
    writer =
      Task.async(fn ->
        for i <- 1..40, do: append_on_peer(ctx, "w#{i}", broadcast: false)
      end)

    take = fn ->
      {:ok, access} = Access.channel(ctx.owner.id, ctx.channel.id)
      Lumia.Snapshots.channel(access)
    end

    snapshots =
      Stream.repeatedly(take)
      |> Enum.reduce_while([], fn snapshot, acc ->
        if Task.yield(writer, 5), do: {:halt, [snapshot | acc]}, else: {:cont, [snapshot | acc]}
      end)

    assert length(snapshots) >= 2

    assert Enum.any?(snapshots, &(&1.seq in 1..39)),
           "no snapshot landed while writes were in flight"

    for snapshot <- snapshots do
      in_snapshot = Enum.map(snapshot.messages, & &1.seq)
      assert Enum.all?(in_snapshot, &(&1 <= snapshot.seq))
      after_it = Enum.map(Events.events_after(ctx.channel.id, snapshot.seq, 1_000), & &1.seq)
      assert Enum.sort(in_snapshot ++ after_it) == Enum.to_list(1..40)
    end
  end

  @tag failure_injection: [identity: 8]
  test "an identity proof used on one node is refused on the other", ctx do
    %{identity: identity, device: device, roster: roster} = bootstrap()

    first = proof(identity, device, roster, :SIGN_IN)
    assert {:ok, _, _} = Identity.verify_proof(first, :SIGN_IN)

    assert {:error, :identity_proof_invalid} =
             PeerNode.call(ctx.peer, Identity, :verify_proof, [first, :SIGN_IN])

    second = proof(identity, device, roster, :SIGN_IN)
    assert {:ok, _, _} = PeerNode.call(ctx.peer, Identity, :verify_proof, [second, :SIGN_IN])
    assert {:error, :identity_proof_invalid} = Identity.verify_proof(second, :SIGN_IN)

    # One row per challenge: the replay failed on the insert, not before it.
    assert Repo.aggregate(Lumia.Identity.UsedChallenge, :count) == 2
  end
end
