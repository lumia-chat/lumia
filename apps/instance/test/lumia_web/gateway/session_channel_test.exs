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
    {:ok, socket} = connect(Socket, %{"access_token" => tokens.access_token})
    {:ok, server} = Servers.create_server(account.id, "Acme")
    {:ok, channel} = Servers.create_channel(server.id, account.id, "general")

    %{
      account: account,
      device_id: device_id,
      session: session,
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

  describe "connect and join" do
    test "rejects a bad token" do
      assert :error = connect(Socket, %{"access_token" => "nope"})
    end

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

    test "a cursor more than 1,000 behind gets SNAPSHOT_REQUIRED", ctx do
      {:ok, _} =
        Lumia.Repo.query("UPDATE channels SET last_seq = 2000 WHERE id = $1", [
          Ecto.UUID.dump!(ctx.channel.id)
        ])

      {_, socket} = join!(ctx.socket)

      assert %V1.ChannelSubscribeResult{status: :SNAPSHOT_REQUIRED, last_seq: 2000} =
               subscribe(socket, ctx.channel.id, 0)

      refute_push "frame", _
    end

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

    test "a non-member cannot subscribe and learns nothing", ctx do
      outsider = account("outsider")
      {:ok, _, tokens} = Auth.create_session(outsider.id, Ecto.UUID.generate())
      {:ok, socket} = connect(Socket, %{"access_token" => tokens.access_token})
      {_, socket} = join!(socket)

      assert %V1.ChannelSubscribeResult{status: :ERROR, error: %V1.Error{code: :NOT_FOUND}} =
               subscribe(socket, ctx.channel.id, 0)
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
      {:ok, socket} = connect(Socket, %{"access_token" => tokens.access_token})
      {_, socket} = join!(socket)
      send_frame(socket, {:command, send_message(ctx.channel.id)})
      assert_push "frame", %V1.ServerFrame{frame: {:error, %V1.Error{code: :PERMISSION_DENIED}}}
    end
  end

  describe "revocation" do
    test "a kicked member's subscription is dropped on the account-log event", ctx do
      member = account("member")
      {:ok, _} = Servers.add_member(ctx.server.id, member.id, ctx.account.id)
      Lumia.Workers.ViewerDiff.diff_channel(ctx.channel.id)

      {:ok, _, tokens} = Auth.create_session(member.id, Ecto.UUID.generate())
      {:ok, socket} = connect(Socket, %{"access_token" => tokens.access_token})
      {_, socket} = join!(socket)
      subscribe(socket, ctx.channel.id, 0)

      :ok = Servers.remove_member(ctx.server.id, member.id, ctx.account.id, "removed")

      actor = %{account_id: ctx.account.id, device_id: ctx.device_id}
      {:ok, _} = Events.append(ctx.channel.id, actor, send_message(ctx.channel.id, "after kick"))
      refute_push "frame", {:event_bytes, _}
    end

    test "an acl_version change without a membership event is caught on the next event", ctx do
      member = account("member")
      {:ok, _} = Servers.add_member(ctx.server.id, member.id, ctx.account.id)
      {:ok, _, tokens} = Auth.create_session(member.id, Ecto.UUID.generate())
      {:ok, socket} = connect(Socket, %{"access_token" => tokens.access_token})
      {_, socket} = join!(socket)
      subscribe(socket, ctx.channel.id, 0)

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

    test "session revocation disconnects the socket", ctx do
      {_, _socket} = join!(ctx.socket)
      :ok = Auth.revoke(ctx.session.id)

      assert_push "frame", %V1.ServerFrame{
        frame: {:disconnect, %V1.Disconnect{reason: :DISCONNECT_SESSION_REVOKED}}
      }

      assert_receive {:socket_close, _, _}, 500
    end
  end

  describe "bounded staleness and slow consumers" do
    test "a stale subscription is re-resolved before its next delivery and dropped if view is gone",
         ctx do
      member = account("member")
      {:ok, _} = Servers.add_member(ctx.server.id, member.id, ctx.account.id)
      {:ok, _, tokens} = Auth.create_session(member.id, Ecto.UUID.generate())
      {:ok, socket} = connect(Socket, %{"access_token" => tokens.access_token})
      {_, socket} = join!(socket)
      subscribe(socket, ctx.channel.id, 0)

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

    test "a node joining the cluster re-resolves every subscription", ctx do
      member = account("member")
      {:ok, _} = Servers.add_member(ctx.server.id, member.id, ctx.account.id)
      {:ok, _, tokens} = Auth.create_session(member.id, Ecto.UUID.generate())
      {:ok, socket} = connect(Socket, %{"access_token" => tokens.access_token})
      {_, socket} = join!(socket)
      subscribe(socket, ctx.channel.id, 0)

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

    test "a slow consumer is disconnected and told why", ctx do
      {_, socket} = join!(ctx.socket)
      # The test process is the transport; flood its mailbox past the limit.
      for i <- 1..1_001, do: send(self(), {:junk, i})

      send_frame(socket, {:heartbeat, %V1.Heartbeat{nonce: 1}})

      assert_push "frame", %V1.ServerFrame{
        frame: {:disconnect, %V1.Disconnect{reason: :DISCONNECT_SLOW_CONSUMER}}
      }

      assert_receive {:socket_close, _, _}, 500
      refute_push "frame", %V1.ServerFrame{frame: {:heartbeat, _}}
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
