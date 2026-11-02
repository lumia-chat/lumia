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
      acl_version != cached           → re-resolve; unsubscribe if view is gone
      seq == delivered_seq + 1        → push; advance
      seq >  delivered_seq + 1        → fill the gap from Postgres, then push
  """

  use Phoenix.Channel

  alias Lumia.Accounts
  alias Lumia.Auth
  alias Lumia.Authz.Access
  alias Lumia.Events
  alias Lumia.Instance
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

    socket =
      assign(socket,
        subscriptions: %{},
        account_seq: nil,
        hello: hello
      )

    welcome = %V1.Welcome{
      session_id: Proto.uuid(socket.assigns.session_id),
      account_id: Proto.uuid(socket.assigns.account_id),
      recovery_generation: Instance.recovery_generation(),
      server_time: Proto.timestamp(DateTime.utc_now()),
      heartbeat_interval_seconds: @heartbeat_seconds,
      limits: %V1.Limits{max_subscriptions: @max_subscriptions, max_message_bytes: 16_384}
    }

    {:ok, welcome, socket}
  end

  # ---- Client frames ------------------------------------------------------------

  @impl Phoenix.Channel
  def handle_in("frame", %V1.ClientFrame{frame: {tag, body}}, socket) do
    handle_frame(tag, body, socket)
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
    socket = Enum.reduce(ids, socket, fn id, s -> unsubscribe(s, Proto.from_uuid(id)) end)
    {:noreply, socket}
  end

  defp handle_frame(:resume_account, %V1.ResumeAccount{after_seq: after_seq}, socket) do
    account_id = socket.assigns.account_id
    head = Accounts.head(account_id)

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
      ack = %V1.Ack{
        command_id: command.command_id,
        channel_id: command.channel_id,
        seq: ack.seq,
        event_id: Proto.uuid(ack.event_id)
      }

      {:noreply, push_frame(socket, {:ack, ack})}
    else
      {:error, reason} ->
        {:noreply, push_error(socket, error_code(reason), to_string(reason), command.command_id)}
    end
  end

  defp handle_frame(:high_water_mark, %V1.HighWaterMarkRequest{} = req, socket) do
    ids =
      case req.channel_ids do
        [] -> Map.keys(socket.assigns.subscriptions)
        ids -> Enum.map(ids, &Proto.from_uuid/1)
      end

    heads =
      for id <- ids,
          {seq, _acl} = Events.head(id) || {0, 0},
          do: %V1.Cursor{log_id: Proto.uuid(id), seq: seq}

    account_seq =
      if req.include_account_log, do: Accounts.head(socket.assigns.account_id) || 0, else: 0

    response = %V1.HighWaterMarkResponse{heads: heads, account_log_seq: account_seq}
    {:noreply, push_frame(socket, {:high_water_mark, response})}
  end

  defp handle_frame(:presence, %V1.PresenceUpdate{}, socket), do: {:noreply, socket}
  defp handle_frame(:typing, %V1.Typing{}, socket), do: {:noreply, socket}

  defp handle_frame(_tag, _body, socket) do
    {:noreply, push_error(socket, :INVALID_ARGUMENT, "unexpected frame")}
  end

  # ---- Subscribe and replay ----------------------------------------------------

  defp subscribe_one(%V1.Cursor{log_id: id_bytes, seq: cursor}, socket) do
    channel_id = Proto.from_uuid(id_bytes)

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
        error = %V1.Error{code: error_code(reason), message: to_string(reason)}
        {%V1.ChannelSubscribeResult{channel_id: id_bytes, status: :ERROR, error: error}, socket}
    end
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
        socket = Enum.reduce(page, socket, fn event, s -> deliver(s, channel_id, event, nil) end)
        if length(page) == @replay_page, do: replay(socket, channel_id), else: socket
    end
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

  def handle_info({:nodeup, _node}, socket), do: {:noreply, reresolve_all(socket)}
  def handle_info({:nodedown, _node}, socket), do: {:noreply, socket}

  def handle_info(:slow_consumer, socket) do
    disconnect(socket, :DISCONNECT_SLOW_CONSUMER)
  end

  def handle_info(_other, socket), do: {:noreply, socket}

  defp handle_live(socket, channel_id, sub, event, bytes) do
    cond do
      event.seq <= sub.delivered_seq ->
        socket

      event.acl_version != sub.acl_version or stale?(sub) ->
        case authorize_read(socket, channel_id) do
          {:ok, fresh} ->
            socket
            |> put_subscription(channel_id, %{fresh | delivered_seq: sub.delivered_seq})
            |> handle_live(channel_id, %{fresh | delivered_seq: sub.delivered_seq}, event, bytes)

          {:error, _} ->
            unsubscribe(socket, channel_id)
        end

      event.seq == sub.delivered_seq + 1 ->
        deliver(socket, channel_id, event, bytes)

      true ->
        socket |> replay(channel_id) |> handle_info_dup(channel_id, event, bytes)
    end
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
    unsubscribe(socket, Proto.from_uuid(id))
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
  # Lumia.Authz.Access, shared with the HTTP API.
  defp authorize_read(socket, channel_id) do
    with {:ok, access} <- Access.viewable_channel(socket.assigns.account_id, channel_id) do
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

  defp authorize_write(socket, channel_id, %V1.Command{op: {op, _}}) do
    needed = Map.fetch!(@write_permission, op)

    with {:ok, access} <- Access.require(socket.assigns.account_id, channel_id, needed) do
      {:ok, subscription(access)}
    end
  end

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

  # Every subscription is re-resolved; those that lost view are dropped.
  defp reresolve_all(socket) do
    Enum.reduce(socket.assigns.subscriptions, socket, fn {channel_id, sub}, s ->
      case authorize_read(s, channel_id) do
        {:ok, fresh} ->
          put_subscription(s, channel_id, %{fresh | delivered_seq: sub.delivered_seq})

        {:error, _} ->
          unsubscribe(s, channel_id)
      end
    end)
  end

  defp push_error(socket, code, message, command_id \\ "") do
    push_frame(socket, {:error, %V1.Error{code: code, message: message, command_id: command_id}})
  end

  # The transport process's mailbox is the outbound queue (delivery section 5).
  defp slow_consumer?(socket) do
    case Process.info(socket.transport_pid, :message_queue_len) do
      {:message_queue_len, n} -> n > @slow_consumer_frames
      nil -> false
    end
  end

  defp disconnect(socket, reason) do
    push(socket, "frame", %V1.ServerFrame{frame: {:disconnect, %V1.Disconnect{reason: reason}}})
    {:stop, :normal, socket}
  end

  defp error_code(:not_found), do: :NOT_FOUND
  defp error_code(:permission_denied), do: :PERMISSION_DENIED
  defp error_code(:conflict), do: :CONFLICT
  defp error_code(:channel_not_found), do: :NOT_FOUND
  defp error_code(:message_not_found), do: :NOT_FOUND
  defp error_code(:invalid_command), do: :INVALID_ARGUMENT
  defp error_code(_), do: :INTERNAL
end
