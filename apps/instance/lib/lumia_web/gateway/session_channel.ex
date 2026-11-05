defmodule LumiaWeb.Gateway.SessionChannel do
  @moduledoc """
  The session process: one per WebSocket (docs/specs/delivery.md section 5).

  Holds, per subscribed channel, `delivered_seq`, the cached `acl_version`
  and resolved permissions; the account log cursor; and nothing else. Every
  frame the client sends arrives as the `frame` event; every frame the
  server sends is a push of a `Lumia.V1.ServerFrame` or, for events, the
  stored bytes.

  Delivery rule for a broadcast on a subscribed channel:

      seq <= delivered_seq            → drop (duplicate)
      acl_version != cached, or stale → re-resolve once; unsubscribe if view is gone
      seq == delivered_seq + 1        → push; advance
      seq >  delivered_seq + 1        → fill the gap from Postgres, then push

  Reading a channel over the gateway (subscribe, replay, live events) needs
  `READ_HISTORY` as well as `VIEW_CHANNEL`, the same check as
  `GET /channels/{id}/events`, so the two paths cannot disagree.

  Malformed client input (an empty frame, an id that is not 16 bytes, a
  command with no operation) is answered with `INVALID_ARGUMENT`; it never
  crashes the session.
  """

  use Phoenix.Channel

  alias Lumia.Accounts
  alias Lumia.Auth
  alias Lumia.Authz.Access
  alias Lumia.Events
  alias Lumia.Instance
  alias Lumia.Mls
  alias Lumia.Proto
  alias Lumia.V1

  @replay_page 200
  @replay_cap 1_000
  @heartbeat_seconds 30
  @max_subscriptions 500
  @slow_consumer_frames 1_000
  # A subscription not re-resolved within this window is re-resolved before
  # its next delivery (authorization section 5).
  @staleness_ms :timer.minutes(10)

  # ---- Join ------------------------------------------------------------------

  @impl Phoenix.Channel
  def join("session", %V1.Hello{} = hello, socket) do
    Phoenix.PubSub.subscribe(Lumia.PubSub, Accounts.topic(socket.assigns.account_id))
    Phoenix.PubSub.subscribe(Lumia.PubSub, Auth.topic(socket.assigns.session_id))
    # After a partition heals, every subscription is re-resolved before
    # anything is delivered (delivery section 5).
    :ok = :net_kernel.monitor_nodes(true)

    {:ok, _} =
      LumiaWeb.Presence.track(self(), "devices", socket.assigns.device_id, %{
        account_id: socket.assigns.account_id
      })

    socket =
      assign(socket,
        subscriptions: %{},
        account_seq: nil,
        hello: hello,
        presence: %{status: "ONLINE", custom_status: "", watching: MapSet.new()}
      )

    welcome = %V1.Welcome{
      session_id: Proto.uuid(socket.assigns.session_id),
      account_id: Proto.uuid(socket.assigns.account_id),
      recovery_generation: Instance.recovery_generation(),
      server_time: Proto.timestamp(DateTime.utc_now()),
      heartbeat_interval_seconds: @heartbeat_seconds,
      limits: %V1.Limits{max_subscriptions: @max_subscriptions, max_message_bytes: 16_384},
      key_package_count:
        Mls.key_package_count(socket.assigns.account_id, socket.assigns.device_id)
    }

    {:ok, welcome, socket}
  end

  # ---- Client frames ------------------------------------------------------------

  @impl Phoenix.Channel
  def handle_in("frame", %V1.ClientFrame{frame: {tag, body}}, socket) do
    handle_frame(tag, body, socket)
  end

  def handle_in("frame", _empty_or_unknown, socket) do
    {:noreply, push_error(socket, :INVALID_ARGUMENT, "empty frame")}
  end

  defp handle_frame(:heartbeat, %V1.Heartbeat{} = hb, socket) do
    {:noreply, push_frame(socket, {:heartbeat, hb})}
  end

  defp handle_frame(:subscribe, %V1.Subscribe{cursors: cursors}, socket) do
    if map_size(socket.assigns.subscriptions) + length(cursors) > @max_subscriptions do
      {:noreply, push_error(socket, :RATE_LIMITED, "too many subscriptions")}
    else
      {results, socket} = Enum.map_reduce(cursors, socket, &subscribe_one/2)
      {:noreply, push_frame(socket, {:subscribe_result, %V1.SubscribeResult{results: results}})}
    end
  end

  defp handle_frame(:unsubscribe, %V1.Unsubscribe{channel_ids: ids}, socket) do
    socket =
      ids
      |> Enum.map(&Proto.from_uuid/1)
      |> Enum.reject(&is_nil/1)
      |> Enum.reduce(socket, fn id, s -> unsubscribe(s, id) end)

    {:noreply, socket}
  end

  defp handle_frame(:resume_account, %V1.ResumeAccount{after_seq: after_seq}, socket) do
    account_id = socket.assigns.account_id
    head = Accounts.head(account_id) || 0

    if head - after_seq > @replay_cap do
      result = %V1.AccountResumeResult{status: :SNAPSHOT_REQUIRED, last_seq: head}

      {:noreply,
       socket |> assign(:account_seq, head) |> push_frame({:account_resume_result, result})}
    else
      result = %V1.AccountResumeResult{status: :RESUMED, last_seq: head}
      socket = push_frame(socket, {:account_resume_result, result})
      {:noreply, replay_account(assign(socket, :account_seq, after_seq))}
    end
  end

  defp handle_frame(:command, %V1.Command{} = command, socket) do
    channel_id = Proto.from_uuid(command.channel_id)
    actor = %{account_id: socket.assigns.account_id, device_id: socket.assigns.device_id}

    with {:ok, _sub} <- authorize_write(socket, channel_id, command),
         {:ok, ack} <- Events.append(channel_id, actor, command) do
      :telemetry.execute([:lumia, :gateway, :ack], %{count: 1}, %{})

      ack = %V1.Ack{
        command_id: command.command_id,
        channel_id: command.channel_id,
        seq: ack.seq,
        event_id: Proto.uuid(ack.event_id)
      }

      {:noreply, push_frame(socket, {:ack, ack})}
    else
      {:error, reason} ->
        {:noreply, push_command_error(socket, reason, command.command_id)}
    end
  end

  # Heads are reported for subscribed channels, or for channels the account
  # could subscribe to; a head is a fact about a channel and is not
  # disclosed for one the account cannot see.
  defp handle_frame(:high_water_mark, %V1.HighWaterMarkRequest{} = req, socket) do
    ids =
      case req.channel_ids do
        [] -> Map.keys(socket.assigns.subscriptions)
        ids -> ids |> Enum.map(&Proto.from_uuid/1) |> Enum.reject(&is_nil/1)
      end

    heads =
      for id <- ids,
          readable?(socket, id),
          {seq, _acl} = Events.head(id) || {0, 0},
          do: %V1.Cursor{log_id: Proto.uuid(id), seq: seq}

    account_seq =
      if req.include_account_log, do: Accounts.head(socket.assigns.account_id) || 0, else: 0

    response = %V1.HighWaterMarkResponse{heads: heads, account_log_seq: account_seq}
    {:noreply, push_frame(socket, {:high_water_mark, response})}
  end

  # Presence (protocol 6.7, architecture 3): one Phoenix.Presence topic per
  # server, and this session is tracked in a server's topic only while the
  # client says it has that server on screen; the update replaces the
  # watched set. The topic's diffs come back as PresenceDiff frames, and a
  # newly watched server first gets everyone present as "joined".
  defp handle_frame(:presence, %V1.PresenceUpdate{} = update, socket) do
    current = socket.assigns.presence
    status = presence_status(update.status)
    account_id = socket.assigns.account_id

    wanted =
      update.watch_server_ids
      |> Enum.map(&Proto.from_uuid/1)
      |> Enum.reject(&is_nil/1)
      |> Enum.filter(&Lumia.Servers.member_for(&1, account_id))
      |> MapSet.new()

    meta = %{
      status: status,
      custom_status: update.custom_status,
      device_id: socket.assigns.device_id
    }

    changed = status != current.status or update.custom_status != current.custom_status

    for server_id <- MapSet.difference(current.watching, wanted) do
      LumiaWeb.Presence.untrack(self(), presence_topic(server_id), account_id)
      Phoenix.PubSub.unsubscribe(Lumia.PubSub, presence_topic(server_id))
    end

    if changed do
      for server_id <- MapSet.intersection(current.watching, wanted) do
        {:ok, _} = LumiaWeb.Presence.update(self(), presence_topic(server_id), account_id, meta)
      end
    end

    socket =
      Enum.reduce(MapSet.difference(wanted, current.watching), socket, fn server_id, socket ->
        topic = presence_topic(server_id)
        :ok = Phoenix.PubSub.subscribe(Lumia.PubSub, topic)
        {:ok, _} = LumiaWeb.Presence.track(self(), topic, account_id, meta)

        joined =
          topic
          |> LumiaWeb.Presence.list()
          |> Enum.map(fn {id, %{metas: metas}} -> presence_entry(id, metas) end)

        push_frame(
          socket,
          {:presence_diff, %V1.PresenceDiff{server_id: Proto.uuid(server_id), joined: joined}}
        )
      end)

    presence = %{status: status, custom_status: update.custom_status, watching: wanted}
    {:noreply, assign(socket, presence: presence)}
  end

  defp handle_frame(:typing, %V1.Typing{}, socket), do: {:noreply, socket}

  defp handle_frame(_tag, _body, socket) do
    {:noreply, push_error(socket, :INVALID_ARGUMENT, "unexpected frame")}
  end

  defp readable?(socket, channel_id) do
    Map.has_key?(socket.assigns.subscriptions, channel_id) or
      match?({:ok, _}, authorize_read(socket, channel_id))
  end

  # ---- Subscribe and replay ----------------------------------------------------

  defp subscribe_one(%V1.Cursor{log_id: id_bytes, seq: cursor}, socket) do
    case Proto.from_uuid(id_bytes) do
      nil -> {subscribe_error(id_bytes, :invalid_argument), socket}
      channel_id -> subscribe_channel(channel_id, id_bytes, cursor, socket)
    end
  end

  defp subscribe_channel(channel_id, id_bytes, cursor, socket) do
    case authorize_read(socket, channel_id) do
      {:ok, sub} ->
        {head, _} = Events.head(channel_id)

        result = %V1.ChannelSubscribeResult{
          channel_id: id_bytes,
          last_seq: head,
          acl_version: sub.acl_version
        }

        if head - cursor > @replay_cap do
          socket = put_subscription(socket, channel_id, %{sub | delivered_seq: head})
          {%{result | status: :SNAPSHOT_REQUIRED}, socket}
        else
          socket = put_subscription(socket, channel_id, %{sub | delivered_seq: cursor})
          {%{result | status: :RESUMED}, replay(socket, channel_id)}
        end

      {:error, reason} ->
        {subscribe_error(id_bytes, reason), socket}
    end
  end

  defp subscribe_error(id_bytes, reason) do
    error = %V1.Error{code: error_code(reason), message: to_string(reason)}
    %V1.ChannelSubscribeResult{channel_id: id_bytes, status: :ERROR, error: error}
  end

  defp put_subscription(socket, channel_id, sub) do
    unless Map.has_key?(socket.assigns.subscriptions, channel_id) do
      Phoenix.PubSub.subscribe(Lumia.PubSub, Events.topic(channel_id))
    end

    assign(socket, :subscriptions, Map.put(socket.assigns.subscriptions, channel_id, sub))
  end

  defp unsubscribe(socket, channel_id) do
    Phoenix.PubSub.unsubscribe(Lumia.PubSub, Events.topic(channel_id))
    assign(socket, :subscriptions, Map.delete(socket.assigns.subscriptions, channel_id))
  end

  # One loop for subscribe replay, live gaps and reconciliation nudges
  # (delivery section 6): page events after delivered_seq until a short page.
  defp replay(socket, channel_id) do
    case socket.assigns.subscriptions[channel_id] do
      nil ->
        socket

      sub ->
        page = Events.events_after(channel_id, sub.delivered_seq, @replay_page)
        moved? = Enum.any?(page, &(&1.acl_version > sub.acl_version))

        # The same rule as live delivery (authorization section 5): a page
        # that carries a newer acl_version, or a stale subscription, is
        # re-resolved before anything from it is delivered. This is how a
        # change made on another node during a partition takes effect here
        # when reconciliation brings the events it was committed with.
        if page != [] and (moved? or stale?(sub)),
          do: replay_reauthorized(socket, channel_id, sub, page),
          else: deliver_page(socket, channel_id, page)
    end
  end

  defp replay_reauthorized(socket, channel_id, sub, page) do
    case authorize_read(socket, channel_id) do
      {:ok, fresh} ->
        socket
        |> put_subscription(channel_id, %{fresh | delivered_seq: sub.delivered_seq})
        |> deliver_page(channel_id, page)

      {:error, _} ->
        unsubscribe(socket, channel_id)
    end
  end

  defp deliver_page(socket, channel_id, page) do
    socket = Enum.reduce(page, socket, fn event, s -> deliver(s, channel_id, event, nil) end)
    if length(page) == @replay_page, do: replay(socket, channel_id), else: socket
  end

  defp replay_account(socket) do
    page =
      Accounts.events_after(socket.assigns.account_id, socket.assigns.account_seq, @replay_page)

    socket =
      Enum.reduce(page, socket, fn event, s ->
        s |> push_frame({:account_event, event}) |> assign(:account_seq, event.seq)
      end)

    if length(page) == @replay_page, do: replay_account(socket), else: socket
  end

  # ---- Live delivery -----------------------------------------------------------

  @impl Phoenix.Channel
  def handle_info({:event, %V1.Event{} = event, bytes}, socket) do
    channel_id = Proto.from_uuid(event.channel_id)

    case socket.assigns.subscriptions[channel_id] do
      nil -> {:noreply, socket}
      sub -> {:noreply, handle_live(socket, channel_id, sub, event, bytes)}
    end
  end

  def handle_info({:head, channel_id, head_seq}, socket) do
    case socket.assigns.subscriptions[channel_id] do
      %{delivered_seq: delivered} when head_seq > delivered ->
        {:noreply, replay(socket, channel_id)}

      _ ->
        {:noreply, socket}
    end
  end

  def handle_info({:account_event, %V1.AccountEvent{} = event, _bytes}, socket) do
    socket = apply_account_event(socket, event)

    case socket.assigns.account_seq do
      nil ->
        {:noreply, socket}

      seq when event.seq <= seq ->
        {:noreply, socket}

      seq when event.seq == seq + 1 ->
        {:noreply,
         socket |> push_frame({:account_event, event}) |> assign(:account_seq, event.seq)}

      _ ->
        {:noreply, replay_account(socket)}
    end
  end

  def handle_info({:account_head, head_seq}, socket) do
    case socket.assigns.account_seq do
      seq when is_integer(seq) and head_seq > seq -> {:noreply, replay_account(socket)}
      _ -> {:noreply, socket}
    end
  end

  def handle_info({:session_revoked, _id}, socket) do
    disconnect(socket, :DISCONNECT_SESSION_REVOKED)
  end

  # A newer connection for the same session took over (protocol 6.1).
  def handle_info({:session_superseded, _id}, socket) do
    disconnect(socket, :DISCONNECT_PROTOCOL_ERROR)
  end

  # Every subscription is re-resolved before its next delivery, by the
  # staleness rule rather than all at once: a node joining the cluster
  # must not make every session on this node query Postgres in the same
  # instant.
  def handle_info({:nodeup, _node}, socket), do: {:noreply, mark_all_stale(socket)}
  def handle_info({:nodedown, _node}, socket), do: {:noreply, socket}

  def handle_info(:slow_consumer, socket) do
    disconnect(socket, :DISCONNECT_SLOW_CONSUMER)
  end

  def handle_info(
        %Phoenix.Socket.Broadcast{
          event: "presence_diff",
          topic: "presence:server:" <> server_id = topic,
          payload: %{joins: joins, leaves: leaves}
        },
        socket
      ) do
    present = LumiaWeb.Presence.list(topic)

    {updated, joined} =
      joins
      |> Enum.map(fn {id, %{metas: metas}} -> presence_entry(id, metas) end)
      |> Enum.split_with(fn entry -> Map.has_key?(leaves, Proto.from_uuid(entry.account_id)) end)

    left =
      leaves
      |> Map.keys()
      |> Enum.reject(&(Map.has_key?(joins, &1) or Map.has_key?(present, &1)))
      |> Enum.map(&Proto.uuid/1)

    diff = %V1.PresenceDiff{
      server_id: Proto.uuid(server_id),
      joined: joined,
      updated: updated,
      left_account_ids: left
    }

    if joined == [] and updated == [] and left == [] do
      {:noreply, socket}
    else
      {:noreply, push_frame(socket, {:presence_diff, diff})}
    end
  end

  def handle_info(_other, socket), do: {:noreply, socket}

  defp handle_live(socket, channel_id, sub, event, bytes) do
    cond do
      event.seq <= sub.delivered_seq ->
        socket

      event.acl_version != sub.acl_version or stale?(sub) ->
        case authorize_read(socket, channel_id) do
          {:ok, fresh} ->
            fresh = %{fresh | delivered_seq: sub.delivered_seq}

            socket
            |> put_subscription(channel_id, fresh)
            |> deliver_live(channel_id, fresh, event, bytes)

          {:error, _} ->
            unsubscribe(socket, channel_id)
        end

      true ->
        deliver_live(socket, channel_id, sub, event, bytes)
    end
  end

  # After the permission check has passed for this event: in order means
  # push; a gap means fill from Postgres first. An event whose acl_version
  # is older than the one just resolved is still delivered: the account
  # holds the view now, and re-resolving again could not change that.
  defp deliver_live(socket, channel_id, sub, event, bytes) do
    if event.seq == sub.delivered_seq + 1,
      do: deliver(socket, channel_id, event, bytes),
      else: socket |> replay(channel_id) |> handle_info_dup(channel_id, event, bytes)
  end

  # After a gap fill the live event is usually already delivered; the
  # delivery rule's duplicate check handles it.
  defp handle_info_dup(socket, channel_id, event, bytes) do
    case socket.assigns.subscriptions[channel_id] do
      %{delivered_seq: d} when event.seq == d + 1 -> deliver(socket, channel_id, event, bytes)
      _ -> socket
    end
  end

  defp deliver(socket, channel_id, %V1.Event{} = event, bytes) do
    payload = if bytes, do: {:event_bytes, bytes}, else: {:event_bytes, V1.Event.encode(event)}
    socket = push_raw(socket, payload)
    sub = socket.assigns.subscriptions[channel_id]

    assign(
      socket,
      :subscriptions,
      Map.put(socket.assigns.subscriptions, channel_id, %{sub | delivered_seq: event.seq})
    )
  end

  # Membership changes on the account log act on subscriptions immediately
  # (authorization section 5); the acl_version check is the backstop.
  defp apply_account_event(socket, %V1.AccountEvent{
         payload: {:channel_removed, %V1.ChannelRemoved{channel_id: id}}
       }) do
    case Proto.from_uuid(id) do
      nil -> socket
      channel_id -> unsubscribe(socket, channel_id)
    end
  end

  defp apply_account_event(socket, %V1.AccountEvent{
         payload: {:membership_removed, %V1.MembershipRemoved{server_id: server_id}}
       }) do
    server_id = Proto.from_uuid(server_id)

    socket.assigns.subscriptions
    |> Enum.filter(fn {_, sub} -> sub.server_id == server_id end)
    |> Enum.reduce(socket, fn {channel_id, _}, s -> unsubscribe(s, channel_id) end)
  end

  defp apply_account_event(socket, _event), do: socket

  # ---- Authorization -----------------------------------------------------------

  # A subscription record: what the delivery rule needs. Authoritative via
  # Lumia.Authz.Access, shared with the HTTP API: reading the log over the
  # gateway takes the same permission as GET /channels/{id}/events.
  defp authorize_read(socket, channel_id) do
    with {:ok, access} <- Access.require(socket.assigns.account_id, channel_id, :read_history) do
      {:ok, subscription(access)}
    end
  end

  # The permission each command op requires.
  @write_permission %{
    send_message: :send_messages,
    edit_message: :send_messages,
    delete_message: :send_messages,
    add_reaction: :add_reactions,
    remove_reaction: :add_reactions,
    pin_message: :manage_messages,
    unpin_message: :manage_messages,
    send_encrypted: :send_messages
  }

  defp authorize_write(_socket, nil, _command), do: {:error, :invalid_argument}

  defp authorize_write(socket, channel_id, %V1.Command{op: {op, _}}) do
    with {:ok, needed} <- Map.fetch(@write_permission, op),
         {:ok, access} <- Access.require(socket.assigns.account_id, channel_id, needed) do
      {:ok, subscription(access)}
    else
      :error -> {:error, :invalid_argument}
      {:error, _} = e -> e
    end
  end

  defp authorize_write(_socket, _channel_id, %V1.Command{}), do: {:error, :invalid_argument}

  defp subscription(access) do
    %{
      server_id: access.server_id,
      acl_version: access.acl_version,
      permissions: access.permissions,
      delivered_seq: 0,
      resolved_at: System.monotonic_time(:millisecond)
    }
  end

  # ---- Pushing -----------------------------------------------------------------

  defp presence_topic(server_id), do: "presence:server:" <> server_id

  defp presence_status(:PRESENCE_STATUS_UNSPECIFIED), do: "ONLINE"
  defp presence_status(:OFFLINE), do: "ONLINE"
  defp presence_status(status) when is_atom(status), do: Atom.to_string(status)
  defp presence_status(_), do: "ONLINE"

  # One entry per account: the most present of its devices' statuses.
  defp presence_entry(account_id, metas) do
    rank = %{"ONLINE" => 0, "AWAY" => 1, "DO_NOT_DISTURB" => 2}

    best =
      Enum.min_by(metas, &Map.get(rank, &1.status, 3), fn ->
        %{status: "ONLINE", custom_status: ""}
      end)

    %V1.Presence{
      account_id: Proto.uuid(account_id),
      status: String.to_existing_atom(best.status),
      custom_status: best.custom_status || ""
    }
  end

  defp push_frame(socket, {tag, body}) do
    push_raw(socket, %V1.ServerFrame{frame: {tag, body}})
  end

  defp push_raw(socket, payload) do
    if slow_consumer?(socket) do
      # Stop from a callback that can return {:stop, ...}; drop this frame,
      # the client resumes from its cursors after reconnecting.
      send(self(), :slow_consumer)
      socket
    else
      push(socket, "frame", payload)
      socket
    end
  end

  defp stale?(%{resolved_at: at}), do: System.monotonic_time(:millisecond) - at > @staleness_ms

  # Every subscription is re-resolved before its next delivery.
  defp mark_all_stale(socket) do
    past = System.monotonic_time(:millisecond) - @staleness_ms - 1

    assign(
      socket,
      :subscriptions,
      Map.new(socket.assigns.subscriptions, fn {id, sub} -> {id, %{sub | resolved_at: past}} end)
    )
  end

  defp push_command_error(socket, {:failed_precondition, state}, command_id) do
    push_error(socket, :FAILED_PRECONDITION, "failed precondition", command_id, %{
      "state" => state
    })
  end

  defp push_command_error(socket, reason, command_id) do
    push_error(socket, error_code(reason), to_string(reason), command_id)
  end

  defp push_error(socket, code, message, command_id \\ "", details \\ %{}) do
    error = %V1.Error{code: code, message: message, command_id: command_id, details: details}
    push_frame(socket, {:error, error})
  end

  # The transport process's mailbox is the outbound queue (delivery section 5).
  defp slow_consumer?(socket) do
    case Process.info(socket.transport_pid, :message_queue_len) do
      {:message_queue_len, n} -> n > @slow_consumer_frames
      nil -> false
    end
  end

  # Tells the client why, then closes the WebSocket. Stopping the channel
  # alone would leave the socket open and make Phoenix report the exit as
  # `phx_close`, which the serializer renders as a second Disconnect with
  # the wrong reason. The transport closes on this message exactly as it
  # does for an endpoint `broadcast(socket_id, "disconnect", %{})`; sent
  # from this process after the push, it arrives after the frame.
  defp disconnect(socket, reason) do
    push(socket, "frame", %V1.ServerFrame{frame: {:disconnect, %V1.Disconnect{reason: reason}}})

    send(socket.transport_pid, %Phoenix.Socket.Broadcast{
      topic: socket.id || "session",
      event: "disconnect",
      payload: %{}
    })

    {:stop, :normal, socket}
  end

  defp error_code(:not_found), do: :NOT_FOUND
  defp error_code(:permission_denied), do: :PERMISSION_DENIED
  defp error_code(:conflict), do: :CONFLICT
  defp error_code(:channel_not_found), do: :NOT_FOUND
  defp error_code(:message_not_found), do: :NOT_FOUND
  defp error_code(:invalid_command), do: :INVALID_ARGUMENT
  defp error_code(:invalid_argument), do: :INVALID_ARGUMENT
  defp error_code({:failed_precondition, _}), do: :FAILED_PRECONDITION
  defp error_code(_), do: :INTERNAL
end
