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

  # A session idle for this long hibernates (Phoenix's own default is
  # 15 s). The connect burst leaves a watcher of a busy server's presence
  # with hundreds of KB of heap, and nothing a session does needs it kept
  # warm: a wake costs one collection of a small heap.
  use Phoenix.Channel, hibernate_after: 5_000

  alias Lumia.Accounts
  alias Lumia.Auth
  alias Lumia.Authz.Access
  alias Lumia.Authz.Permission
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
  # Typing (protocol 6.7): how far ahead `until` is set, and how soon after
  # one Typing frame the next for the same channel is fanned out. Clients
  # send one every 5 s; a faster one is dropped.
  @typing_seconds 8
  @typing_spacing_ms 3_000

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
        presence: %{status: "ONLINE", custom_status: "", watching: MapSet.new()},
        typing: nil
      )

    welcome = %V1.Welcome{
      session_id: Proto.uuid(socket.assigns.session_id),
      account_id: Proto.uuid(socket.assigns.account_id),
      recovery_generation: Instance.recovery_generation(),
      server_time: Proto.timestamp(DateTime.utc_now()),
      heartbeat_interval_seconds: @heartbeat_seconds,
      limits: %V1.Limits{
        max_subscriptions: @max_subscriptions,
        max_message_bytes: Lumia.Events.max_message_bytes()
      },
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
      {results, socket} = subscribe_all(cursors, socket)
      socket = push_frame(socket, {:subscribe_result, %V1.SubscribeResult{results: results}})
      # The burst leaves garbage behind that would otherwise be most of
      # what the session costs until it hibernates on its own.
      {:noreply, socket, :hibernate}
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

  # Typing (protocol 6.7) is fanned out on the channel's topic to the other
  # sessions subscribed to it, and is nothing else: no row, no event, no
  # answer. It is taken only from a session subscribed to the channel, for
  # an account that may post there, and never for a personal conversation,
  # where the instance is not to learn who is writing to whom. The check is
  # the subscription's cached view (at most the staleness window old), not
  # a fresh resolve: what it guards is a hint that somebody is writing.
  defp handle_frame(:typing, %V1.Typing{channel_id: id}, socket) do
    channel_id = Proto.from_uuid(id)
    now = System.monotonic_time(:millisecond)

    with %{server_id: server_id, permissions: bits} when server_id != nil <-
           socket.assigns.subscriptions[channel_id],
         true <- Permission.has?(bits, :send_messages),
         false <- typing_too_soon?(socket.assigns.typing, channel_id, now) do
      indicator = %V1.TypingIndicator{
        channel_id: id,
        account_id: Proto.uuid(socket.assigns.account_id),
        until: Proto.timestamp(DateTime.add(DateTime.utc_now(), @typing_seconds, :second))
      }

      Phoenix.PubSub.broadcast_from(
        Lumia.PubSub,
        self(),
        Events.topic(channel_id),
        {:typing, channel_id, socket.assigns.account_id, indicator}
      )

      {:noreply, assign(socket, :typing, {channel_id, now})}
    else
      _ -> {:noreply, socket}
    end
  end

  defp handle_frame(_tag, _body, socket) do
    {:noreply, push_error(socket, :INVALID_ARGUMENT, "unexpected frame")}
  end

  defp typing_too_soon?({channel_id, at}, channel_id, now), do: now - at < @typing_spacing_ms
  defp typing_too_soon?(_last, _channel_id, _now), do: false

  defp readable?(socket, channel_id) do
    Map.has_key?(socket.assigns.subscriptions, channel_id) or
      match?({:ok, _}, authorize_read(socket, channel_id))
  end

  # ---- Subscribe and replay ----------------------------------------------------

  # The whole frame is resolved at once (one channel read, one membership
  # read per server), and the head comes from the channel row that read
  # brought, so a subscribe at the head costs no page read. The topics are
  # subscribed before the heads are read: an event committed in between is
  # in the head and its broadcast a duplicate, so nothing is missed; the
  # topics of channels that end up refused are dropped again.
  defp subscribe_all(cursors, socket) do
    parsed = Enum.map(cursors, &{&1, Proto.from_uuid(&1.log_id)})
    ids = for {_, id} <- parsed, id != nil, do: id
    new_ids = Enum.uniq(ids -- Map.keys(socket.assigns.subscriptions))
    Enum.each(new_ids, &Phoenix.PubSub.subscribe(Lumia.PubSub, Events.topic(&1)))
    access = Access.require_many(socket.assigns.account_id, ids, :read_history)

    {results, socket} =
      Enum.map_reduce(parsed, socket, fn
        {%V1.Cursor{log_id: bytes}, nil}, socket ->
          {subscribe_error(bytes, :invalid_argument), socket}

        {%V1.Cursor{log_id: bytes, seq: cursor}, id}, socket ->
          case access[id] do
            {:ok, a} -> subscribe_channel(id, bytes, cursor, subscription(a), socket)
            {:error, reason} -> {subscribe_error(bytes, reason), socket}
          end
      end)

    for id <- new_ids, not Map.has_key?(socket.assigns.subscriptions, id) do
      Phoenix.PubSub.unsubscribe(Lumia.PubSub, Events.topic(id))
    end

    {results, socket}
  end

  defp subscribe_channel(channel_id, id_bytes, cursor, sub, socket) do
    head = sub.head

    result = %V1.ChannelSubscribeResult{
      channel_id: id_bytes,
      last_seq: head,
      acl_version: sub.acl_version
    }

    sub = Map.delete(sub, :head)

    cond do
      head - cursor > @replay_cap ->
        socket = store_subscription(socket, channel_id, %{sub | delivered_seq: head})
        {%{result | status: :SNAPSHOT_REQUIRED}, socket}

      cursor >= head ->
        # Nothing to read; the live rule covers what arrives from here.
        socket = store_subscription(socket, channel_id, %{sub | delivered_seq: cursor})
        {%{result | status: :RESUMED}, socket}

      true ->
        socket = store_subscription(socket, channel_id, %{sub | delivered_seq: cursor})
        {%{result | status: :RESUMED}, replay(socket, channel_id)}
    end
  end

  # The topic is already subscribed (subscribe_all); only the record is kept.
  defp store_subscription(socket, channel_id, sub) do
    assign(socket, :subscriptions, Map.put(socket.assigns.subscriptions, channel_id, sub))
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

  # Someone is typing in a channel this session is subscribed to. Not shown
  # to the account's own other devices, and not delivered on a subscription
  # that is due a re-resolve: the next event settles whether the view is
  # still held, and a hint is not worth a query.
  def handle_info({:typing, channel_id, account_id, %V1.TypingIndicator{} = indicator}, socket) do
    case socket.assigns.subscriptions[channel_id] do
      %{} = sub when account_id != socket.assigns.account_id ->
        if stale?(sub),
          do: {:noreply, socket},
          else: {:noreply, push_frame(socket, {:typing, indicator})}

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
    {updated, joined} =
      joins
      |> Enum.map(fn {id, %{metas: metas}} -> presence_entry(id, metas) end)
      |> Enum.split_with(fn entry -> Map.has_key?(leaves, Proto.from_uuid(entry.account_id)) end)

    # An account has left only when none of its devices remains: one
    # lookup per leaving key, never a listing of the topic, which every
    # watcher would otherwise build on every diff of a connect burst.
    left =
      leaves
      |> Map.keys()
      |> Enum.reject(&(Map.has_key?(joins, &1) or LumiaWeb.Presence.get_by_key(topic, &1) != []))
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
    # The author's own, or a moderator's who may not be in the channel:
    # `Access.check_command/3` says which.
    delete_message: :view_channel,
    add_reaction: :add_reactions,
    remove_reaction: :add_reactions,
    pin_message: :manage_messages,
    unpin_message: :manage_messages,
    send_encrypted: :send_messages
  }

  defp authorize_write(_socket, nil, _command), do: {:error, :invalid_argument}

  defp authorize_write(socket, channel_id, %V1.Command{op: {op, _}} = command) do
    with {:ok, needed} <- Map.fetch(@write_permission, op),
         {:ok, access} <- Access.require(socket.assigns.account_id, channel_id, needed),
         :ok <- Access.check_command(access, socket.assigns.account_id, command) do
      {:ok, subscription(access)}
    else
      :error -> {:error, :invalid_argument}
      {:error, _} = e -> e
    end
  end

  defp authorize_write(_socket, _channel_id, %V1.Command{}), do: {:error, :invalid_argument}

  # `head` is the channel row's last_seq at resolution, read by the
  # subscribe and not kept.
  defp subscription(access) do
    %{
      server_id: access.server_id,
      acl_version: access.acl_version,
      permissions: access.permissions,
      delivered_seq: 0,
      resolved_at: System.monotonic_time(:millisecond),
      head: access.channel.last_seq
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

  defp push_command_error(socket, {:payload_too_large, max}, command_id) do
    push_error(socket, :PAYLOAD_TOO_LARGE, "payload too large", command_id, %{
      "max" => Integer.to_string(max)
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
