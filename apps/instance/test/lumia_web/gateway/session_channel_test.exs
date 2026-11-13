defmodule LumiaWeb.Gateway.SessionChannelTest do
  use LumiaWeb.ChannelCase, async: true

  import Ecto.Query, only: [from: 2]
  import Lumia.Fixtures

  alias Lumia.Auth
  alias Lumia.Events
  alias Lumia.Proto
  alias Lumia.Servers
  alias Lumia.V1
  alias LumiaWeb.Gateway.Reconciler
  alias LumiaWeb.Gateway.SessionChannel
  alias LumiaWeb.Gateway.Socket

  setup do
    account = account()
    device_id = Ecto.UUID.generate()
    {:ok, session, tokens} = Auth.create_session(account.id, device_id)
    {:ok, socket} = connect(Socket, %{}, connect_info: %{auth_token: tokens.access_token})
    {:ok, server} = Servers.create_server(account.id, "Acme")
    {:ok, channel} = Servers.create_channel(server.id, account.id, "general")

    %{
      account: account,
      device_id: device_id,
      session: session,
      tokens: tokens,
      socket: socket,
      server: server,
      channel: channel
    }
  end

  defp join!(socket) do
    hello = %V1.Hello{client: "test", device_id: Proto.uuid(socket.assigns.device_id)}

    {:ok, %V1.Welcome{} = welcome, socket} =
      subscribe_and_join(socket, SessionChannel, "session", hello)

    {welcome, socket}
  end

  defp send_frame(socket, {tag, body}),
    do: push(socket, "frame", %V1.ClientFrame{frame: {tag, body}})

  defp subscribe(socket, channel_id, cursor) do
    send_frame(
      socket,
      {:subscribe,
       %V1.Subscribe{cursors: [%V1.Cursor{log_id: Proto.uuid(channel_id), seq: cursor}]}}
    )

    assert_push "frame", %V1.ServerFrame{
      frame: {:subscribe_result, %V1.SubscribeResult{results: [result]}}
    }

    result
  end

  defp event_seq({:event_bytes, bytes}), do: V1.Event.decode(bytes).seq

  # Where a channel's log is now: a member's arrival in a default channel
  # posts "joined" there (architecture 6.8), which a subscribe at 0 replays.
  defp head(channel_id), do: elem(Events.head(channel_id), 0)

  defp cursors(channels),
    do: Enum.map(channels, &%V1.Cursor{log_id: Proto.uuid(&1.id), seq: 0})

  # Only the session's own queries count: the other tests of the suite run
  # their own in the same VM (and the handler must not raise on a query
  # without a source, which would detach it).
  def count_query(_event, _measurements, metadata, {pid, session}) do
    if self() == session, do: send(pid, {:query, Map.get(metadata, :source)})
  end

  defp flush_queries(count \\ 0) do
    receive do
      {:query, _} -> flush_queries(count + 1)
    after
      0 -> count
    end
  end

  describe "presence" do
    test "watching a server lists who is there, and their comings and goings follow", ctx do
      {_welcome, a} = join!(ctx.socket)
      other = account("other")
      {:ok, _} = Servers.add_member(ctx.server.id, other.id, ctx.account.id)
      other_device = Ecto.UUID.generate()
      {:ok, _session, tokens} = Auth.create_session(other.id, other_device)
      {:ok, b_socket} = connect(Socket, %{}, connect_info: %{auth_token: tokens.access_token})
      {_welcome, b} = join!(b_socket)

      watch = %V1.PresenceUpdate{status: :ONLINE, watch_server_ids: [Proto.uuid(ctx.server.id)]}
      send_frame(a, {:presence, watch})
      me = Proto.uuid(ctx.account.id)

      assert_push "frame", %V1.ServerFrame{
        frame:
          {:presence_diff,
           %V1.PresenceDiff{joined: [%V1.Presence{account_id: ^me, status: :ONLINE}]}}
      }

      # B arrives in the server's topic: A hears it; B's own first diff names both.
      send_frame(b, {:presence, watch})
      them = Proto.uuid(other.id)

      assert_push "frame", %V1.ServerFrame{
        frame: {:presence_diff, %V1.PresenceDiff{joined: [%V1.Presence{account_id: ^them}]}}
      }

      # B goes away: A hears the update.
      send_frame(b, {:presence, %{watch | status: :AWAY}})

      assert_push "frame", %V1.ServerFrame{
        frame:
          {:presence_diff,
           %V1.PresenceDiff{updated: [%V1.Presence{account_id: ^them, status: :AWAY}]}}
      }

      # B stops watching (nothing of the server on screen): A hears it leave.
      send_frame(b, {:presence, %{watch | watch_server_ids: []}})

      assert_push "frame", %V1.ServerFrame{
        frame: {:presence_diff, %V1.PresenceDiff{left_account_ids: [^them]}}
      }
    end

    test "a server the account is not in is never watched", ctx do
      {_welcome, a} = join!(ctx.socket)
      stranger = account("stranger")
      {:ok, theirs} = Servers.create_server(stranger.id, "Theirs")

      send_frame(
        a,
        {:presence,
         %V1.PresenceUpdate{status: :ONLINE, watch_server_ids: [Proto.uuid(theirs.id)]}}
      )

      refute_push "frame", %V1.ServerFrame{frame: {:presence_diff, _}}
      assert LumiaWeb.Presence.list("presence:server:" <> theirs.id) == %{}
    end
  end

  describe "typing" do
    # A second member of the server, joined and subscribed to its channel.
    defp second_member(ctx) do
      other = account("other")
      {:ok, _} = Servers.add_member(ctx.server.id, other.id, ctx.account.id)
      {:ok, _session, tokens} = Auth.create_session(other.id, Ecto.UUID.generate())
      {:ok, socket} = connect(Socket, %{}, connect_info: %{auth_token: tokens.access_token})
      {_welcome, socket} = join!(socket)
      subscribe(socket, ctx.channel.id, 0)
      {other, socket}
    end

    defp typing(channel_id), do: {:typing, %V1.Typing{channel_id: Proto.uuid(channel_id)}}

    test "is fanned out to the channel's other subscribers, until a few seconds ahead", ctx do
      {_welcome, a} = join!(ctx.socket)
      subscribe(a, ctx.channel.id, 0)
      {_other, _b} = second_member(ctx)

      send_frame(a, typing(ctx.channel.id))
      channel_id = Proto.uuid(ctx.channel.id)
      me = Proto.uuid(ctx.account.id)

      assert_push "frame", %V1.ServerFrame{
        frame:
          {:typing, %V1.TypingIndicator{channel_id: ^channel_id, account_id: ^me, until: until}}
      }

      ahead = DateTime.diff(Proto.from_timestamp(until), DateTime.utc_now(), :millisecond)
      assert ahead > 6_000 and ahead <= 8_000
      # One push: to the other subscriber, and none back to the one typing.
      refute_push "frame", %V1.ServerFrame{frame: {:typing, _}}
    end

    test "is not shown to the account's own other device", ctx do
      {_welcome, a} = join!(ctx.socket)
      subscribe(a, ctx.channel.id, 0)
      {:ok, _session, tokens} = Auth.create_session(ctx.account.id, Ecto.UUID.generate())
      {:ok, mine} = connect(Socket, %{}, connect_info: %{auth_token: tokens.access_token})
      {_welcome, mine} = join!(mine)
      subscribe(mine, ctx.channel.id, 0)

      send_frame(a, typing(ctx.channel.id))
      _ = :sys.get_state(mine.channel_pid)
      refute_push "frame", %V1.ServerFrame{frame: {:typing, _}}
    end

    test "a second frame for the channel within three seconds is dropped", ctx do
      {_welcome, a} = join!(ctx.socket)
      subscribe(a, ctx.channel.id, 0)
      {_other, _b} = second_member(ctx)

      send_frame(a, typing(ctx.channel.id))
      assert_push "frame", %V1.ServerFrame{frame: {:typing, _}}
      send_frame(a, typing(ctx.channel.id))
      refute_push "frame", %V1.ServerFrame{frame: {:typing, _}}

      # Another channel is another matter: the person moved.
      {:ok, design} = Servers.create_channel(ctx.server.id, ctx.account.id, "design")
      send_frame(a, {:subscribe, %V1.Subscribe{cursors: cursors([design])}})
      assert_push "frame", %V1.ServerFrame{frame: {:subscribe_result, _}}
      Phoenix.PubSub.subscribe(Lumia.PubSub, Events.topic(design.id))
      send_frame(a, typing(design.id))
      assert_receive {:typing, _channel, _account, %V1.TypingIndicator{}}
    end

    test "is taken only from a subscriber who may post there", ctx do
      {_welcome, a} = join!(ctx.socket)
      {_other, b} = second_member(ctx)
      Phoenix.PubSub.subscribe(Lumia.PubSub, Events.topic(ctx.channel.id))

      # Not subscribed: nothing, and no error to tell a channel exists.
      send_frame(a, typing(ctx.channel.id))
      send_frame(a, typing(Ecto.UUID.generate()))
      send_frame(a, {:typing, %V1.Typing{channel_id: "short"}})
      _ = :sys.get_state(a.channel_pid)
      refute_receive {:typing, _, _, _}, 50
      refute_push "frame", %V1.ServerFrame{frame: {:error, _}}

      # Subscribed, but posting is denied to everyone but the owner.
      everyone =
        Lumia.Repo.get_by!(Lumia.Servers.Role, server_id: ctx.server.id, is_everyone: true)

      :ok =
        Servers.set_overwrite(ctx.channel.id, ctx.account.id, {:role, everyone.id}, [], [
          :send_messages
        ])

      # The change's own event is what makes the subscription resolve again.
      assert_push "frame", {:event_bytes, _acl_changed}
      send_frame(b, typing(ctx.channel.id))
      _ = :sys.get_state(b.channel_pid)
      refute_receive {:typing, _, _, _}, 50
    end

    test "is never fanned out for a personal conversation", ctx do
      %{channel: dm} = personal_dm(ctx.account, nil)
      {_welcome, a} = join!(ctx.socket)
      subscribe(a, dm.id, 0)
      Phoenix.PubSub.subscribe(Lumia.PubSub, Events.topic(dm.id))

      send_frame(a, typing(dm.id))
      _ = :sys.get_state(a.channel_pid)
      refute_receive {:typing, _, _, _}, 50
    end
  end

  describe "connect and join" do
    test "rejects a bad token" do
      assert :error = connect(Socket, %{}, connect_info: %{auth_token: "nope"})
    end

    @tag failure_injection: [delivery: 7]
    test "Welcome carries the recovery generation and heartbeat interval", %{
      socket: socket,
      session: session
    } do
      {welcome, _socket} = join!(socket)
      assert welcome.recovery_generation == Lumia.Instance.recovery_generation()
      assert welcome.heartbeat_interval_seconds == 30
      assert welcome.session_id == Proto.uuid(session.id)
    end

    test "heartbeats are echoed", %{socket: socket} do
      {_, socket} = join!(socket)
      send_frame(socket, {:heartbeat, %V1.Heartbeat{nonce: 9}})
      assert_push "frame", %V1.ServerFrame{frame: {:heartbeat, %V1.Heartbeat{nonce: 9}}}
    end
  end

  describe "subscribe, replay, live delivery" do
    test "replays from the cursor then delivers live events in order", ctx do
      actor = %{account_id: ctx.account.id, device_id: ctx.device_id}

      for i <- 1..3,
          do:
            {:ok, _} = Events.append(ctx.channel.id, actor, send_message(ctx.channel.id, "m#{i}"))

      {_, socket} = join!(ctx.socket)

      assert %V1.ChannelSubscribeResult{status: :RESUMED, last_seq: 3} =
               subscribe(socket, ctx.channel.id, 1)

      assert_push "frame", {:event_bytes, b2}
      assert_push "frame", {:event_bytes, b3}
      assert V1.Event.decode(b2).seq == 2
      assert V1.Event.decode(b3).seq == 3

      {:ok, _} = Events.append(ctx.channel.id, actor, send_message(ctx.channel.id, "live"))
      assert_push "frame", {:event_bytes, b4}

      assert %V1.Event{seq: 4, payload: {:message_created, %{message: %{body: "live"}}}} =
               V1.Event.decode(b4)
    end

    @tag failure_injection: [delivery: 2]
    test "a gap in live delivery is filled from Postgres in order", ctx do
      actor = %{account_id: ctx.account.id, device_id: ctx.device_id}
      {_, socket} = join!(ctx.socket)
      subscribe(socket, ctx.channel.id, 0)

      # Commit two events with broadcast suppressed, then a third that broadcasts.
      {:ok, _} =
        Events.append(ctx.channel.id, actor, send_message(ctx.channel.id), broadcast: false)

      {:ok, _} =
        Events.append(ctx.channel.id, actor, send_message(ctx.channel.id), broadcast: false)

      {:ok, _} = Events.append(ctx.channel.id, actor, send_message(ctx.channel.id))

      assert_push "frame", p1
      assert_push "frame", p2
      assert_push "frame", p3
      assert Enum.map([p1, p2, p3], &event_seq/1) == [1, 2, 3]
      refute_push "frame", _
    end

    test "duplicates are dropped", ctx do
      {_, socket} = join!(ctx.socket)
      subscribe(socket, ctx.channel.id, 0)

      {:ok, _} =
        Events.append_system(ctx.channel.id, {:acl_changed, %V1.AclChanged{acl_version: 1}})

      assert_push "frame", {:event_bytes, bytes}
      # Re-broadcast the same event; the session must not push it again.
      Events.broadcast(ctx.channel.id, V1.Event.decode(bytes), bytes)
      refute_push "frame", _
    end

    @tag failure_injection: [delivery: 5]
    test "a cursor 1,001 behind gets SNAPSHOT_REQUIRED; 1,000 behind replays", ctx do
      actor = %{account_id: ctx.account.id, device_id: ctx.device_id}

      for i <- 1..1_001 do
        {:ok, _} =
          Events.append(ctx.channel.id, actor, send_message(ctx.channel.id, "m#{i}"),
            broadcast: false
          )
      end

      {_, socket} = join!(ctx.socket)

      assert %V1.ChannelSubscribeResult{status: :SNAPSHOT_REQUIRED, last_seq: 1001} =
               subscribe(socket, ctx.channel.id, 0)

      refute_push "frame", _

      assert %V1.ChannelSubscribeResult{status: :RESUMED} = subscribe(socket, ctx.channel.id, 1)

      for seq <- 2..1_001 do
        assert_push "frame", {:event_bytes, bytes}
        assert V1.Event.decode(bytes).seq == seq
      end

      refute_push "frame", _
    end

    @tag failure_injection: [delivery: 1]
    test "a reconciliation nudge fills what the broadcast missed", ctx do
      actor = %{account_id: ctx.account.id, device_id: ctx.device_id}
      {_, socket} = join!(ctx.socket)
      subscribe(socket, ctx.channel.id, 0)

      {:ok, _} =
        Events.append(ctx.channel.id, actor, send_message(ctx.channel.id), broadcast: false)

      refute_push "frame", _

      assert Reconciler.tick(DateTime.add(DateTime.utc_now(), -10, :second)) >= 1
      assert_push "frame", {:event_bytes, bytes}
      assert V1.Event.decode(bytes).seq == 1
    end

    test "one frame resolves many channels in order, each with its own answer", ctx do
      actor = %{account_id: ctx.account.id, device_id: ctx.device_id}
      {:ok, second} = Servers.create_channel(ctx.server.id, ctx.account.id, "second")
      %{channel: dm} = personal_dm(ctx.account, nil)
      %{channel: not_mine, a: stranger} = personal_dm()
      {:ok, _} = Events.append(second.id, actor, send_message(second.id, "m1"))

      {_, socket} = join!(ctx.socket)

      cursors = [
        %V1.Cursor{log_id: Proto.uuid(ctx.channel.id), seq: 0},
        %V1.Cursor{log_id: "short", seq: 0},
        %V1.Cursor{log_id: Proto.uuid(second.id), seq: 0},
        %V1.Cursor{log_id: Proto.uuid(dm.id), seq: 0},
        %V1.Cursor{log_id: Proto.uuid(not_mine.id), seq: 0},
        %V1.Cursor{log_id: Proto.uuid(Ecto.UUID.generate()), seq: 0}
      ]

      send_frame(socket, {:subscribe, %V1.Subscribe{cursors: cursors}})

      assert_push "frame", %V1.ServerFrame{
        frame: {:subscribe_result, %V1.SubscribeResult{results: results}}
      }

      assert Enum.map(results, & &1.channel_id) == Enum.map(cursors, & &1.log_id)

      assert [
               %{status: :RESUMED, last_seq: 0},
               %{status: :ERROR, error: %V1.Error{code: :INVALID_ARGUMENT}},
               %{status: :RESUMED, last_seq: 1},
               %{status: :RESUMED, last_seq: 0},
               %{status: :ERROR, error: %V1.Error{code: :NOT_FOUND}},
               %{status: :ERROR, error: %V1.Error{code: :NOT_FOUND}}
             ] = results

      # The second channel's one event replays; the others have nothing.
      assert_push "frame", {:event_bytes, bytes}
      assert V1.Event.decode(bytes).seq == 1
      refute_push "frame", _

      # Every accepted channel is live, the refused ones are not.
      {:ok, _} = Events.append(dm.id, actor, send_encrypted(dm.id, :APPLICATION, 0))
      assert_push "frame", {:event_bytes, dm_bytes}
      assert V1.Event.decode(dm_bytes).channel_id == Proto.uuid(dm.id)
      other = %{account_id: stranger.id, device_id: ctx.device_id}
      {:ok, _} = Events.append(not_mine.id, other, send_encrypted(not_mine.id, :APPLICATION, 0))
      refute_push "frame", _
    end

    test "a subscribe at the head reads no page and still delivers what comes next", ctx do
      actor = %{account_id: ctx.account.id, device_id: ctx.device_id}
      {:ok, _} = Events.append(ctx.channel.id, actor, send_message(ctx.channel.id, "m1"))
      {_, socket} = join!(ctx.socket)

      :telemetry.attach(
        ctx.test,
        [:lumia, :repo, :query],
        &__MODULE__.count_query/4,
        {self(), socket.channel_pid}
      )

      assert %V1.ChannelSubscribeResult{status: :RESUMED, last_seq: 1} =
               subscribe(socket, ctx.channel.id, 1)

      refute_push "frame", _
      refute_received {:query, "events"}

      {:ok, _} = Events.append(ctx.channel.id, actor, send_message(ctx.channel.id, "m2"))
      assert_push "frame", {:event_bytes, bytes}
      assert V1.Event.decode(bytes).seq == 2
    end

    test "a subscribe costs the same queries for two channels of a server as for six", ctx do
      channels =
        for name <- ~w(c1 c2 c3 c4 c5 c6) do
          {:ok, channel} = Servers.create_channel(ctx.server.id, ctx.account.id, name)
          channel
        end

      # Warm the ACL cache as a running instance's would be.
      {_, warm} = join!(ctx.socket)
      send_frame(warm, {:subscribe, %V1.Subscribe{cursors: cursors(channels)}})
      assert_push "frame", %V1.ServerFrame{frame: {:subscribe_result, _}}

      {:ok, _, tokens} = Auth.create_session(ctx.account.id, Ecto.UUID.generate())
      {:ok, socket} = connect(Socket, %{}, connect_info: %{auth_token: tokens.access_token})
      {_, socket} = join!(socket)

      :telemetry.attach(
        ctx.test,
        [:lumia, :repo, :query],
        &__MODULE__.count_query/4,
        {self(), socket.channel_pid}
      )

      flush_queries()

      send_frame(socket, {:subscribe, %V1.Subscribe{cursors: cursors(Enum.take(channels, 2))}})
      assert_push "frame", %V1.ServerFrame{frame: {:subscribe_result, _}}
      two = flush_queries()

      send_frame(socket, {:subscribe, %V1.Subscribe{cursors: cursors(Enum.drop(channels, 2))}})
      assert_push "frame", %V1.ServerFrame{frame: {:subscribe_result, _}}
      four = flush_queries()

      # The channels, the membership of the server and of the channels, and the
      # heads come with the channel rows.
      assert two == four
      assert two <= 5
    end

    test "a non-member cannot subscribe and learns nothing", ctx do
      outsider = account("outsider")
      {:ok, _, tokens} = Auth.create_session(outsider.id, Ecto.UUID.generate())
      {:ok, socket} = connect(Socket, %{}, connect_info: %{auth_token: tokens.access_token})
      {_, socket} = join!(socket)

      assert %V1.ChannelSubscribeResult{status: :ERROR, error: %V1.Error{code: :NOT_FOUND}} =
               subscribe(socket, ctx.channel.id, 0)
    end

    test "a member without READ_HISTORY cannot subscribe, the same as the HTTP reads", ctx do
      member = account("member")
      {:ok, _} = Servers.add_member(ctx.server.id, member.id, ctx.account.id)

      everyone =
        Lumia.Repo.get_by!(Lumia.Servers.Role, server_id: ctx.server.id, is_everyone: true)

      :ok =
        Servers.set_overwrite(ctx.channel.id, ctx.account.id, {:role, everyone.id}, [], [
          :read_history
        ])

      actor = %{account_id: ctx.account.id, device_id: ctx.device_id}
      {:ok, _} = Events.append(ctx.channel.id, actor, send_message(ctx.channel.id, "secret"))

      {:ok, _, tokens} = Auth.create_session(member.id, Ecto.UUID.generate())
      {:ok, socket} = connect(Socket, %{}, connect_info: %{auth_token: tokens.access_token})
      {_, socket} = join!(socket)

      assert %V1.ChannelSubscribeResult{
               status: :ERROR,
               error: %V1.Error{code: :PERMISSION_DENIED}
             } = subscribe(socket, ctx.channel.id, 0)

      refute_push "frame", {:event_bytes, _}
    end

    test "high-water marks are not disclosed for channels the account cannot read", ctx do
      outsider = account("outsider")
      {:ok, _, tokens} = Auth.create_session(outsider.id, Ecto.UUID.generate())
      {:ok, socket} = connect(Socket, %{}, connect_info: %{auth_token: tokens.access_token})
      {_, socket} = join!(socket)

      send_frame(
        socket,
        {:high_water_mark,
         %V1.HighWaterMarkRequest{channel_ids: [Proto.uuid(ctx.channel.id), "short"]}}
      )

      assert_push "frame", %V1.ServerFrame{
        frame: {:high_water_mark, %V1.HighWaterMarkResponse{heads: []}}
      }
    end

    test "malformed frames are answered, not crashed on", ctx do
      {_, socket} = join!(ctx.socket)

      push(socket, "frame", %V1.ClientFrame{frame: nil})
      assert_push "frame", %V1.ServerFrame{frame: {:error, %V1.Error{code: :INVALID_ARGUMENT}}}

      # An id that is not 16 bytes.
      send_frame(socket, {:subscribe, %V1.Subscribe{cursors: [%V1.Cursor{log_id: "x", seq: 0}]}})

      assert_push "frame", %V1.ServerFrame{
        frame:
          {:subscribe_result,
           %V1.SubscribeResult{
             results: [
               %V1.ChannelSubscribeResult{
                 status: :ERROR,
                 error: %V1.Error{code: :INVALID_ARGUMENT}
               }
             ]
           }}
      }

      send_frame(socket, {:unsubscribe, %V1.Unsubscribe{channel_ids: ["x"]}})

      # A command with no operation, and one with a bad channel id.
      send_frame(socket, {:command, %V1.Command{command_id: Proto.uuid(Ecto.UUID.generate())}})
      assert_push "frame", %V1.ServerFrame{frame: {:error, %V1.Error{code: :INVALID_ARGUMENT}}}

      bad = %{send_message(ctx.channel.id) | channel_id: "nope"}
      send_frame(socket, {:command, bad})
      assert_push "frame", %V1.ServerFrame{frame: {:error, %V1.Error{code: :INVALID_ARGUMENT}}}

      empty_id = %{send_message(ctx.channel.id) | command_id: ""}
      send_frame(socket, {:command, empty_id})
      assert_push "frame", %V1.ServerFrame{frame: {:error, %V1.Error{code: :INVALID_ARGUMENT}}}

      # Still alive and serving.
      send_frame(socket, {:heartbeat, %V1.Heartbeat{nonce: 5}})
      assert_push "frame", %V1.ServerFrame{frame: {:heartbeat, %V1.Heartbeat{nonce: 5}}}
    end
  end

  describe "commands" do
    test "a command is appended, acked, and delivered back", ctx do
      {_, socket} = join!(ctx.socket)
      subscribe(socket, ctx.channel.id, 0)
      cmd = send_message(ctx.channel.id, "hi")
      send_frame(socket, {:command, cmd})

      assert_push "frame", %V1.ServerFrame{frame: {:ack, %V1.Ack{seq: 1, command_id: command_id}}}
      assert command_id == cmd.command_id
      assert_push "frame", {:event_bytes, bytes}
      assert %V1.Event{seq: 1, command_id: ^command_id} = V1.Event.decode(bytes)
    end

    test "a member without SEND_MESSAGES is denied", ctx do
      member = account("member")
      {:ok, _} = Servers.add_member(ctx.server.id, member.id, ctx.account.id)

      everyone =
        Lumia.Repo.get_by!(Lumia.Servers.Role, server_id: ctx.server.id, is_everyone: true)

      :ok =
        Servers.set_overwrite(ctx.channel.id, ctx.account.id, {:role, everyone.id}, [], [
          :send_messages
        ])

      {:ok, _, tokens} = Auth.create_session(member.id, Ecto.UUID.generate())
      {:ok, socket} = connect(Socket, %{}, connect_info: %{auth_token: tokens.access_token})
      {_, socket} = join!(socket)
      send_frame(socket, {:command, send_message(ctx.channel.id)})
      assert_push "frame", %V1.ServerFrame{frame: {:error, %V1.Error{code: :PERMISSION_DENIED}}}
    end
  end

  describe "revocation" do
    @tag failure_injection: [delivery: 3, authorization: 3]
    test "a kicked member's subscription is dropped on the account-log event", ctx do
      member = account("member")
      {:ok, _} = Servers.add_member(ctx.server.id, member.id, ctx.account.id)
      Lumia.Workers.ViewerDiff.diff_channel(ctx.channel.id)

      {:ok, _, tokens} = Auth.create_session(member.id, Ecto.UUID.generate())
      {:ok, socket} = connect(Socket, %{}, connect_info: %{auth_token: tokens.access_token})
      {_, socket} = join!(socket)
      subscribe(socket, ctx.channel.id, head(ctx.channel.id))

      :ok = Servers.remove_member(ctx.server.id, member.id, ctx.account.id, "removed")

      actor = %{account_id: ctx.account.id, device_id: ctx.device_id}
      {:ok, _} = Events.append(ctx.channel.id, actor, send_message(ctx.channel.id, "after kick"))
      refute_push "frame", {:event_bytes, _}
    end

    @tag failure_injection: [authorization: 4]
    test "an acl_version change without a membership event is caught on the next event", ctx do
      member = account("member")
      {:ok, _} = Servers.add_member(ctx.server.id, member.id, ctx.account.id)
      {:ok, _, tokens} = Auth.create_session(member.id, Ecto.UUID.generate())
      {:ok, socket} = connect(Socket, %{}, connect_info: %{auth_token: tokens.access_token})
      {_, socket} = join!(socket)
      subscribe(socket, ctx.channel.id, head(ctx.channel.id))

      # Make the channel private; the bump appends acl_changed, which the
      # member must not receive because they can no longer view.
      everyone =
        Lumia.Repo.get_by!(Lumia.Servers.Role, server_id: ctx.server.id, is_everyone: true)

      :ok =
        Servers.set_overwrite(ctx.channel.id, ctx.account.id, {:role, everyone.id}, [], [
          :view_channel
        ])

      refute_push "frame", {:event_bytes, _}
    end

    test "session revocation disconnects the socket and closes the transport", ctx do
      {_, _socket} = join!(ctx.socket)
      :ok = Auth.revoke(ctx.session.id)

      assert_push "frame", %V1.ServerFrame{
        frame: {:disconnect, %V1.Disconnect{reason: :DISCONNECT_SESSION_REVOKED}}
      }

      # The transport (this process, under ChannelTest) is told to close,
      # after the frame, so the client never sees a second Disconnect.
      assert_receive %Phoenix.Socket.Broadcast{event: "disconnect"}
      assert_receive {:socket_close, _, _}, 500
    end

    test "a second connection for the same session closes the first with PROTOCOL_ERROR", ctx do
      {_, _first} = join!(ctx.socket)
      {:ok, _session, tokens} = Auth.create_session(ctx.account.id, ctx.device_id)

      # A fresh connection for *another* session leaves the first alone.
      {:ok, _} = connect(Socket, %{}, connect_info: %{auth_token: tokens.access_token})
      refute_push "frame", %V1.ServerFrame{frame: {:disconnect, _}}

      # The same session connecting again supersedes the first.
      {:ok, second} = connect(Socket, %{}, connect_info: %{auth_token: ctx.tokens.access_token})

      assert_push "frame", %V1.ServerFrame{
        frame: {:disconnect, %V1.Disconnect{reason: :DISCONNECT_PROTOCOL_ERROR}}
      }

      assert_receive {:socket_close, _, _}, 500

      # The second connection joins and works normally.
      {_, second} = join!(second)
      send_frame(second, {:heartbeat, %V1.Heartbeat{nonce: 2}})
      assert_push "frame", %V1.ServerFrame{frame: {:heartbeat, %V1.Heartbeat{nonce: 2}}}
    end
  end

  describe "bounded staleness and slow consumers" do
    test "a stale subscription is re-resolved before its next delivery and dropped if view is gone",
         ctx do
      member = account("member")
      {:ok, _} = Servers.add_member(ctx.server.id, member.id, ctx.account.id)
      {:ok, _, tokens} = Auth.create_session(member.id, Ecto.UUID.generate())
      {:ok, socket} = connect(Socket, %{}, connect_info: %{auth_token: tokens.access_token})
      {_, socket} = join!(socket)
      subscribe(socket, ctx.channel.id, head(ctx.channel.id))

      # Remove the member directly in the database, without the account-log
      # event or an acl bump, so only staleness can catch it.
      Lumia.Repo.delete_all(
        from(m in Lumia.Servers.Member,
          where: m.server_id == ^ctx.server.id and m.account_id == ^member.id
        )
      )

      # Age the subscription past the window.
      channel_id = ctx.channel.id

      :sys.replace_state(socket.channel_pid, fn state ->
        # Monotonic time can be negative on the BEAM, so age relative to now.
        stale = System.monotonic_time(:millisecond) - :timer.minutes(11)
        update_in(state.assigns.subscriptions[channel_id].resolved_at, fn _ -> stale end)
      end)

      actor = %{account_id: ctx.account.id, device_id: ctx.device_id}
      {:ok, _} = Events.append(ctx.channel.id, actor, send_message(ctx.channel.id, "after"))
      refute_push "frame", {:event_bytes, _}
    end

    test "a node joining the cluster marks every subscription stale, so each is re-resolved before its next delivery",
         ctx do
      member = account("member")
      {:ok, _} = Servers.add_member(ctx.server.id, member.id, ctx.account.id)
      {:ok, _, tokens} = Auth.create_session(member.id, Ecto.UUID.generate())
      {:ok, socket} = connect(Socket, %{}, connect_info: %{auth_token: tokens.access_token})
      {_, socket} = join!(socket)
      subscribe(socket, ctx.channel.id, head(ctx.channel.id))

      Lumia.Repo.delete_all(
        from(m in Lumia.Servers.Member,
          where: m.server_id == ^ctx.server.id and m.account_id == ^member.id
        )
      )

      send(socket.channel_pid, {:nodeup, :other@host})
      _ = :sys.get_state(socket.channel_pid)

      actor = %{account_id: ctx.account.id, device_id: ctx.device_id}
      {:ok, _} = Events.append(ctx.channel.id, actor, send_message(ctx.channel.id, "after"))
      refute_push "frame", {:event_bytes, _}
    end

    @tag failure_injection: [delivery: 6]
    test "a slow consumer is disconnected and told why, and resumes cleanly from its cursors",
         ctx do
      actor = %{account_id: ctx.account.id, device_id: ctx.device_id}
      {_, socket} = join!(ctx.socket)
      subscribe(socket, ctx.channel.id, 0)
      {:ok, _} = Events.append(ctx.channel.id, actor, send_message(ctx.channel.id, "before"))
      assert_push "frame", {:event_bytes, bytes}
      assert V1.Event.decode(bytes).seq == 1

      # The test process is the transport; flood its mailbox past the limit.
      for i <- 1..1_001, do: send(self(), {:junk, i})

      send_frame(socket, {:heartbeat, %V1.Heartbeat{nonce: 1}})

      assert_push "frame", %V1.ServerFrame{
        frame: {:disconnect, %V1.Disconnect{reason: :DISCONNECT_SLOW_CONSUMER}}
      }

      assert_receive {:socket_close, _, _}, 500
      refute_push "frame", %V1.ServerFrame{frame: {:heartbeat, _}}

      # The client drains, reconnects on the same session, and resumes from
      # the cursor it held: nothing is replayed twice, nothing is lost.
      for _ <- 1..1_001, do: receive(do: ({:junk, _} -> :ok))

      {:ok, again} =
        connect(Socket, %{}, connect_info: %{auth_token: ctx.tokens.access_token})

      {_, again} = join!(again)
      assert %V1.ChannelSubscribeResult{status: :RESUMED} = subscribe(again, ctx.channel.id, 1)
      refute_push "frame", {:event_bytes, _}

      {:ok, _} = Events.append(ctx.channel.id, actor, send_message(ctx.channel.id, "after"))
      assert_push "frame", {:event_bytes, bytes}
      assert V1.Event.decode(bytes).seq == 2
      refute_push "frame", _
    end
  end

  describe "account log" do
    test "resumes and delivers live account events", ctx do
      {_, socket} = join!(ctx.socket)
      send_frame(socket, {:resume_account, %V1.ResumeAccount{after_seq: 0}})

      assert_push "frame", %V1.ServerFrame{
        frame: {:account_resume_result, %V1.AccountResumeResult{status: :RESUMED}}
      }

      {:ok, _} =
        Lumia.Accounts.append_event(ctx.account.id, {:profile_updated, %V1.ProfileUpdated{}})

      assert_push "frame", %V1.ServerFrame{
        frame: {:account_event, %V1.AccountEvent{payload: {:profile_updated, _}}}
      }
    end
  end
end
