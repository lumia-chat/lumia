defmodule Mix.Tasks.Lumia.Load do
  @shortdoc "The first load test: the rows of architecture section 17 the instance can measure"

  @moduledoc """
  Measures, against a seeded database and the real gateway, the rows of
  architecture section 17 that belong to the instance:

    * send to delivered at the other connected clients (p99, 100 ms), with
      the row's workload: a 1 KB body, a 50-member channel, background
      writes instance-wide, a large database;
    * gateway connect, authenticate and resume 50 channels with no gaps,
      presence on (p99, 500 ms);
    * scrollback of 50 messages (p99, 50 ms);
    * the protobuf frame overhead of a plain message (64 bytes);
    * concurrent connections per node, with `--connections`: 50
      subscriptions each, a tenth in presence, one message a second per
      thousand connections, and the memory the node spends per connection;

  and the two intervals architecture section 20 left to measurement: the
  cost of one reconciler tick (delivery section 7) over every seeded
  channel having moved, and the cost of one re-resolution (authorization
  section 5), which the bounded-staleness interval is priced against.

      LUMIA_CODE_RELOADER=false mix lumia.load
          [--channels 1000] [--events 1000000] [--members 50]
                     [--messages 300] [--rate 10] [--background 10]
                     [--resumes 30] [--scrollbacks 300] [--connections 0] [--ramp 200]
                     [--ci]

  The connections row measured on the server alone, the clients in a VM of
  their own (the clients' sockets and buffers would otherwise be counted):

      LUMIA_CODE_RELOADER=false mix lumia.load --server            # prints its pid, then its memory every 5 s
      LUMIA_CODE_RELOADER=false mix lumia.load --client-only --connections 10000 --server-pid <pid>

  Seeding writes `--events` message events spread over `--channels`
  channels of one server, straight into the tables (no gateway), and is
  skipped when the database already holds a load fixture at least that
  big. `--ci` seeds a small fixture, runs every row but the connections,
  and exits non-zero when a measured p99 is over its budget. Runs against
  the dev database and leaves its rows behind; `mix ecto.reset` clears
  them.

  Not measured here, and said so in the report: TLS, a network RTT, a
  cold Postgres cache (the scrollback requests spread over the fixture
  instead), and the client-side rows (cold start, idle RSS, MLS, which
  `lumia-core` measures as `mls_budget`).
  """

  use Mix.Task

  import Ecto.Query

  alias Lumia.Accounts
  alias Lumia.Accounts.Account
  alias Lumia.Auth
  alias Lumia.Authz.Access
  alias Lumia.Events
  alias Lumia.Events.Channel
  alias Lumia.Events.Event
  alias Lumia.Events.EventPayload
  alias Lumia.Events.Message
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.Servers
  alias Lumia.Servers.Server
  alias Lumia.V1
  alias LumiaWeb.Gateway.Reconciler

  @switches [
    channels: :integer,
    events: :integer,
    members: :integer,
    messages: :integer,
    rate: :integer,
    background: :integer,
    resumes: :integer,
    scrollbacks: :integer,
    connections: :integer,
    ci: :boolean,
    server: :boolean,
    client_only: :boolean,
    server_pid: :integer,
    ramp: :integer
  ]
  @port 4798
  @owner_handle "load-owner"
  @server_name "Load test"
  @body_bytes 1_024
  @resume_channels 50
  @heartbeat_ms 30_000
  @budgets [
    send_p99_ms: 100,
    resume_p99_ms: 500,
    scrollback_p99_ms: 50,
    frame_overhead_bytes: 200
  ]

  @impl Mix.Task
  def run(args) do
    {opts, _} = OptionParser.parse!(args, strict: @switches)
    ci? = Keyword.get(opts, :ci, false)

    opts =
      if ci?,
        do:
          Keyword.merge(
            [channels: 100, events: 100_000, messages: 100, resumes: 20, scrollbacks: 200],
            opts
          ),
        else: opts

    Logger.configure(level: :warning)
    Mix.Task.run("app.config")

    # The reloader plug is compiled into the endpoint (dev.exs) and costs
    # about 8 ms per HTTP request even when nothing changed, which would be
    # most of the scrollback row.
    if Application.get_env(:lumia, LumiaWeb.Endpoint)[:code_reloader] do
      Mix.raise(
        "the dev code reloader is compiled in; run with LUMIA_CODE_RELOADER=false " <>
          "(the endpoint recompiles without it)"
      )
    end

    Application.put_env(
      :lumia,
      LumiaWeb.Endpoint,
      Keyword.merge(Application.get_env(:lumia, LumiaWeb.Endpoint),
        server: true,
        http: [
          ip: {127, 0, 0, 1},
          port: @port,
          thousand_island_options: [num_acceptors: 64, transport_options: [backlog: 4_096]]
        ]
      )
    )

    if opts[:client_only] do
      Application.put_env(
        :lumia,
        LumiaWeb.Endpoint,
        Keyword.put(Application.get_env(:lumia, LumiaWeb.Endpoint), :server, false)
      )
    end

    Mix.Task.run("app.start")

    cond do
      opts[:server] -> server_loop()
      opts[:client_only] -> client_only(opts)
      true -> measure(opts, ci?)
    end
  end

  # `--server`: this VM is only the instance, on the load port, printing its
  # memory every five seconds so a `--client-only` run in another VM can
  # measure what the connections cost the server alone.
  defp server_loop do
    info("server pid #{System.pid()}")

    Stream.interval(5_000)
    |> Enum.each(fn _ ->
      memory = :erlang.memory()

      info(
        "server: rss #{div(rss_kb(System.pid()), 1_024)} MB, vm total #{mb(memory[:total])} MB, " <>
          "processes #{mb(memory[:processes])} MB, binary #{mb(memory[:binary])} MB, " <>
          "ets #{mb(memory[:ets])} MB, #{length(Process.list())} processes" <>
          sample_session() <> ets_tables()
      )
    end)
  end

  # The five largest ETS tables, with the binary bytes inside a sample of
  # their rows: refc binaries stored in a table belong to no process, and
  # the VM's binary total counts them.
  defp ets_tables do
    :ets.all()
    |> Enum.map(fn table -> {table, :ets.info(table, :memory) || 0} end)
    |> Enum.sort_by(&elem(&1, 1), :desc)
    |> Enum.take(5)
    |> Enum.map_join("", fn {table, words} ->
      rows = :ets.info(table, :size)
      sample = Enum.take(:ets.tab2list(table), 100)
      {bytes, largest} = binary_bytes(sample)
      name = :ets.info(table, :name)

      "; ets #{inspect(name)} #{rows} rows #{div(words * 8, 1_024)} KB, " <>
        "#{length(sample)} sampled hold #{div(bytes, 1_024)} KB of binaries (largest #{largest} B)"
    end)
  end

  defp binary_bytes(term) do
    walk(term, {0, 0})
  end

  defp walk(bin, {sum, max}) when is_binary(bin),
    do: {sum + byte_size(bin), max(max, byte_size(bin))}

  defp walk(list, acc) when is_list(list), do: Enum.reduce(list, acc, &walk/2)
  defp walk(tuple, acc) when is_tuple(tuple), do: tuple |> Tuple.to_list() |> walk(acc)
  defp walk(%{} = map, acc), do: map |> Map.to_list() |> walk(acc)
  defp walk(_other, acc), do: acc

  # One session process (found by its process label) and what it links
  # to, its transport among them: heap and the refc binaries each holds,
  # so the per-connection memory can be read by what it is.
  defp sample_session do
    label = {Phoenix.Channel, LumiaWeb.Gateway.SessionChannel, "session"}

    case Enum.find(Process.list(), &(:proc_lib.get_label(&1) == label)) do
      nil -> ""
      pid -> "; " <> describe(pid, "session") <> transport_of(pid)
    end
  end

  defp transport_of(session) do
    case Process.info(session, :links) do
      {:links, links} ->
        links
        |> Enum.filter(&is_pid/1)
        |> Enum.map(&describe(&1, "linked"))
        |> Enum.map_join("", &(", " <> &1))

      _ ->
        ""
    end
  end

  defp describe(pid, label) do
    case Process.info(pid, [:memory, :binary, :message_queue_len]) do
      [memory: memory, binary: refs, message_queue_len: queued] ->
        sizes = Enum.map(refs, &elem(&1, 1))

        "#{label} heap #{div(memory, 1_024)} KB, #{length(refs)} binaries #{div(Enum.sum(sizes), 1_024)} KB " <>
          "(largest #{div(Enum.max(sizes, fn -> 0 end), 1_024)} KB), #{queued} queued"

      _ ->
        "#{label} gone"
    end
  end

  # `--client-only --connections N --server-pid P`: the connections row
  # against a `--server` VM, its memory read from the OS by pid.
  defp client_only(opts) do
    fixture = seed(Keyword.get(opts, :channels, 1_000), Keyword.get(opts, :events, 1_000_000))
    n = Keyword.get(opts, :connections, 1_000)
    server_pid = Keyword.get(opts, :server_pid) || Mix.raise("--server-pid is needed")
    result = connections(fixture, body(), n, server_pid, Keyword.get(opts, :ramp, 200))
    info("\n#{n} connections: #{div(result.server_bytes, 1_024)} KB of server RSS each")
  end

  defp measure(opts, ci?) do
    fixture = seed(Keyword.get(opts, :channels, 1_000), Keyword.get(opts, :events, 1_000_000))
    body = body()

    results = %{
      frame: frame_overhead(fixture),
      send:
        send_to_delivered(
          fixture,
          body,
          Keyword.get(opts, :members, 50),
          Keyword.get(opts, :messages, 300),
          Keyword.get(opts, :rate, 10),
          Keyword.get(opts, :background, 10)
        ),
      resume: connect_and_resume(fixture, Keyword.get(opts, :resumes, 30)),
      scrollback: scrollback(fixture, Keyword.get(opts, :scrollbacks, 300)),
      reconcile: reconciliation(fixture)
    }

    results =
      case Keyword.get(opts, :connections, 0) do
        n when n > 0 and not ci? ->
          Map.put(
            results,
            :connections,
            connections(fixture, body, n, System.pid(), Keyword.get(opts, :ramp, 200))
          )

        _ ->
          results
      end

    summary(results)
    if ci?, do: check_budgets(results)
  end

  # ---- Seed -----------------------------------------------------------------------

  # One server, `channels` text channels, `events` message events spread
  # evenly, inserted in batches straight into the tables with the same rows
  # the write path makes. Reused when a fixture at least this big exists.
  defp seed(channels, events) do
    per_channel = max(div(events, channels), 1)

    case existing_fixture(channels, per_channel) do
      {:ok, fixture} ->
        info(
          "fixture: reusing #{length(fixture.channel_ids)} channels × #{fixture.per_channel} events"
        )

        fixture

      :none ->
        info("fixture: seeding #{channels} channels × #{per_channel} events")
        build_fixture(channels, per_channel)
    end
  end

  defp existing_fixture(channels, per_channel) do
    with %Account{} = owner <- Repo.get_by(Account, handle: @owner_handle),
         %Server{} = server <-
           Repo.one(
             from(s in Server,
               where: s.owner_account_id == ^owner.id and s.name == @server_name,
               order_by: [desc: s.inserted_at],
               limit: 1
             )
           ),
         ids when length(ids) >= channels <-
           Repo.all(
             from(c in Channel,
               where: c.server_id == ^server.id and c.last_seq >= ^per_channel,
               select: c.id,
               order_by: c.position
             )
           ) do
      {:ok, %{owner: owner, server_id: server.id, channel_ids: ids, per_channel: per_channel}}
    else
      _ -> :none
    end
  end

  defp build_fixture(channels, per_channel) do
    owner =
      Repo.get_by(Account, handle: @owner_handle) ||
        elem(Accounts.create_account(%{handle: @owner_handle}), 1)

    {:ok, server} = Servers.create_server(owner.id, @server_name)

    channel_ids =
      for i <- 1..channels do
        {:ok, channel} = Servers.create_channel(server.id, owner.id, "load-#{i}")
        channel.id
      end

    started = System.monotonic_time(:millisecond)
    body = body()

    channel_ids
    |> Enum.with_index()
    |> Task.async_stream(
      fn {channel_id, i} ->
        seed_channel(channel_id, owner.id, per_channel, body)
        if rem(i + 1, 100) == 0, do: info("  seeded #{i + 1} channels")
      end,
      max_concurrency: 8,
      timeout: :infinity,
      ordered: false
    )
    |> Stream.run()

    seconds = (System.monotonic_time(:millisecond) - started) / 1_000
    total = channels * per_channel

    info(
      "fixture: #{total} events in #{Float.round(seconds, 1)} s (#{round(total / max(seconds, 0.001))} events/s)"
    )

    %{owner: owner, server_id: server.id, channel_ids: channel_ids, per_channel: per_channel}
  end

  defp seed_channel(channel_id, owner_id, per_channel, body) do
    now = DateTime.utc_now()

    1..per_channel
    |> Stream.chunk_every(1_000)
    |> Enum.each(fn seqs ->
      rows = Enum.map(seqs, &seed_rows(channel_id, owner_id, &1, body, now))
      Repo.insert_all(Event, Enum.map(rows, &elem(&1, 0)))
      Repo.insert_all(EventPayload, Enum.map(rows, &elem(&1, 1)))
      Repo.insert_all(Message, Enum.map(rows, &elem(&1, 2)))
    end)

    Repo.update_all(from(c in Channel, where: c.id == ^channel_id),
      set: [last_seq: per_channel, last_seq_at: DateTime.truncate(now, :second)]
    )
  end

  defp seed_rows(channel_id, owner_id, seq, body, now) do
    message_id = Ecto.UUID.generate(version: 7)
    event_id = Ecto.UUID.generate(version: 7)

    message = %V1.Message{
      message_id: Proto.uuid(message_id),
      channel_id: Proto.uuid(channel_id),
      seq: seq,
      author_account_id: Proto.uuid(owner_id),
      body: body,
      created_at: Proto.timestamp(now)
    }

    event = %V1.Event{
      event_id: Proto.uuid(event_id),
      channel_id: Proto.uuid(channel_id),
      seq: seq,
      created_at: Proto.timestamp(now),
      acl_version: 1,
      payload: {:message_created, %V1.MessageCreated{message: message}}
    }

    {
      %{
        channel_id: channel_id,
        seq: seq,
        id: event_id,
        kind: "message_created",
        actor_account_id: owner_id,
        command_id: message_id,
        acl_version: 1,
        created_at: now
      },
      %{channel_id: channel_id, seq: seq, payload: V1.Event.encode(event)},
      %{
        id: message_id,
        channel_id: channel_id,
        seq: seq,
        author_account_id: owner_id,
        body: body,
        created_at: now
      }
    }
  end

  defp body, do: :crypto.strong_rand_bytes(div(@body_bytes, 2)) |> Base.encode16(case: :lower)

  # The channels' heads now: the send phases move them between runs, and a
  # cursor at the head is what "no gaps" means.
  defp heads(fixture) do
    Repo.all(from c in Channel, where: c.id in ^fixture.channel_ids, select: {c.id, c.last_seq})
    |> Map.new()
  end

  defp member(fixture) do
    {:ok, account} =
      Accounts.create_account(%{
        handle: "load-#{Base.encode16(:crypto.strong_rand_bytes(6), case: :lower)}"
      })

    {:ok, _} = Servers.add_member(fixture.server_id, account.id, fixture.owner.id)
    account
  end

  defp token(account) do
    {:ok, _, tokens} = Auth.create_session(account.id, Ecto.UUID.generate())
    tokens.access_token
  end

  # ---- Frame overhead -------------------------------------------------------------

  # The bytes of a ServerFrame carrying a message event, less the body: the
  # transport-only budget of section 17.
  defp frame_overhead(fixture) do
    channel_id = hd(fixture.channel_ids)
    now = DateTime.utc_now()

    frame = fn body ->
      message = %V1.Message{
        message_id: Proto.uuid(Ecto.UUID.generate(version: 7)),
        channel_id: Proto.uuid(channel_id),
        seq: 1_234_567,
        author_account_id: Proto.uuid(fixture.owner.id),
        body: body,
        created_at: Proto.timestamp(now)
      }

      event = %V1.Event{
        event_id: Proto.uuid(Ecto.UUID.generate(version: 7)),
        channel_id: Proto.uuid(channel_id),
        seq: 1_234_567,
        command_id: message.message_id,
        actor_account_id: message.author_account_id,
        created_at: Proto.timestamp(now),
        acl_version: 7,
        payload: {:message_created, %V1.MessageCreated{message: message}}
      }

      byte_size(V1.ServerFrame.encode(%V1.ServerFrame{frame: {:event, event}}))
    end

    empty = frame.("")
    with_body = frame.(String.duplicate("x", 1_000))
    overhead = with_body - 1_000

    info("""

    protobuf frame for a plain message
      no body        #{empty} bytes
      1,000-byte body #{with_body} bytes, so #{overhead} bytes of overhead (budget #{@budgets[:frame_overhead_bytes]})
    """)

    %{empty: empty, overhead: overhead}
  end

  # ---- Send to delivered ----------------------------------------------------------

  defp send_to_delivered(fixture, body, members, messages, rate, background) do
    channel_id = hd(fixture.channel_ids)
    others = tl(fixture.channel_ids)
    coordinator = self()
    head = Map.fetch!(heads(fixture), channel_id)

    subscribers =
      for _ <- 1..members do
        account = member(fixture)
        start_client(token(account), coordinator, subscribe: [{channel_id, head}])
      end

    sender = start_client(token(member(fixture)), coordinator, subscribe: [{channel_id, head}])
    wait_for(:subscribed, members + 1)

    writer = background_writer(others, fixture.owner.id, body, background)

    info(
      "\nsend to delivered: #{members} members, #{messages} messages at #{rate}/s, 1 KB bodies, " <>
        "#{background} background writes/s across #{length(others)} other channels"
    )

    sent =
      for i <- 1..messages, into: %{} do
        command = command(channel_id, body)
        sent_at = System.monotonic_time(:microsecond)
        send(sender, {:send, command})
        if i < messages, do: Process.sleep(div(1_000, rate))
        {command.command_id, sent_at}
      end

    samples = collect_deliveries(map_size(sent) * members, sent, [])
    acks = collect_acks(messages, sent, [])
    send(writer, :stop)
    Enum.each([sender | subscribers], &send(&1, :stop))

    delivered =
      report("send to delivered (#{members} subscribers × #{messages} messages)", samples)

    report("send to ack", acks)
    delivered
  end

  # Background load through the real write path (no subscribers on those
  # channels, so the broadcast is one ETS lookup).
  defp background_writer(_channel_ids, _actor, _body, 0), do: spawn(fn -> :ok end)

  defp background_writer(channel_ids, actor, body, per_second) do
    spawn_link(fn ->
      background_loop(
        channel_ids,
        %{account_id: actor, device_id: nil},
        body,
        div(1_000, per_second)
      )
    end)
  end

  defp background_loop(channel_ids, actor, body, sleep_ms) do
    receive do
      :stop -> :ok
    after
      sleep_ms ->
        channel_id = Enum.random(channel_ids)
        {:ok, _} = Events.append(channel_id, actor, command(channel_id, body))
        background_loop(channel_ids, actor, body, sleep_ms)
    end
  end

  defp command(channel_id, body) do
    %V1.Command{
      command_id: Proto.uuid(Ecto.UUID.generate(version: 7)),
      channel_id: Proto.uuid(channel_id),
      op: {:send_message, %V1.SendMessage{body: body}}
    }
  end

  # ---- Connect and resume ---------------------------------------------------------

  # A fresh session each time: TCP connect, upgrade, Hello, Subscribe to 50
  # channels at their heads, and PresenceUpdate watching the server. The
  # time is from before the connect to the SubscribeResult, and to the
  # first PresenceDiff.
  defp connect_and_resume(fixture, iterations) do
    account = member(fixture)

    heads = heads(fixture)
    cursors = fixture.channel_ids |> Enum.take(@resume_channels) |> Enum.map(&{&1, heads[&1]})
    coordinator = self()

    info(
      "\nconnect, authenticate, resume #{length(cursors)} channels with presence: #{iterations} sessions"
    )

    # One session first so the code paths are warm; it is not counted.
    warm =
      start_client(token(account), coordinator, subscribe: cursors, presence: fixture.server_id)

    wait_for(:subscribed, 1)
    send(warm, :stop)

    {resumed, presence} =
      Enum.reduce(1..iterations, {[], []}, fn _, {r, p} ->
        client =
          start_client(token(account), coordinator,
            subscribe: cursors,
            presence: fixture.server_id,
            time_from_connect: true
          )

        resumed_in = wait_for({:resumed_in, client})
        presence_in = wait_for({:presence_in, client})
        send(client, :stop)
        {[resumed_in | r], [presence_in | p]}
      end)

    report("connect to SubscribeResult (#{length(cursors)} channels)", resumed)
    report("connect to the first PresenceDiff", presence)
  end

  # ---- Scrollback -----------------------------------------------------------------

  # GET /channels/{id}/messages?limit=50 over HTTP/1.1 with a keep-alive
  # connection, random channels and offsets across the fixture.
  defp scrollback(fixture, requests) do
    token = token(member(fixture))
    {:ok, conn} = Mint.HTTP.connect(:http, "127.0.0.1", @port, protocols: [:http1])

    headers = [
      {"authorization", "Bearer " <> token},
      {"accept", "application/x-protobuf"}
    ]

    info("\nscrollback: #{requests} requests of 50 messages at random offsets")

    {samples, _conn} =
      Enum.map_reduce(1..requests, conn, fn _, conn ->
        channel_id = Enum.random(fixture.channel_ids)
        before = :rand.uniform(max(fixture.per_channel - 50, 1)) + 50
        path = "/api/v1/channels/#{channel_id}/messages?limit=50&before_seq=#{before}"

        started = System.monotonic_time(:microsecond)
        {:ok, conn, ref} = Mint.HTTP.request(conn, "GET", path, headers, nil)
        {conn, body} = read_response(conn, ref, [])
        elapsed = System.monotonic_time(:microsecond) - started

        %V1.ListMessagesResponse{messages: messages} = V1.ListMessagesResponse.decode(body)
        length(messages) == 50 || Mix.raise("scrollback returned #{length(messages)} messages")
        {elapsed, conn}
      end)

    report("scrollback of 50 messages", samples)
  end

  defp read_response(conn, ref, acc) do
    receive do
      message ->
        case Mint.HTTP.stream(conn, message) do
          {:ok, conn, responses} ->
            acc =
              Enum.reduce(responses, acc, fn
                {:data, ^ref, data}, acc -> [data | acc]
                _, acc -> acc
              end)

            if Enum.any?(responses, &match?({:done, ^ref}, &1)),
              do: {conn, acc |> Enum.reverse() |> IO.iodata_to_binary()},
              else: read_response(conn, ref, acc)

          {:error, _, reason, _} ->
            Mix.raise("scrollback request failed: #{inspect(reason)}")

          # a late message from an earlier phase's client
          :unknown ->
            read_response(conn, ref, acc)
        end
    after
      10_000 -> Mix.raise("scrollback request timed out")
    end
  end

  # ---- Reconciliation and re-resolution ------------------------------------------

  # The tick's cost when every fixture channel moved in the window (the
  # worst case for one node: the query's rows are the instance's active
  # channels), and one re-resolution's cost, which prices the staleness
  # bound: a subscription costs at most one of these per bound per
  # delivery.
  defp reconciliation(fixture) do
    now = DateTime.utc_now() |> DateTime.truncate(:second)

    Repo.update_all(from(c in Channel, where: c.id in ^fixture.channel_ids),
      set: [last_seq_at: now]
    )

    ticks =
      for _ <- 1..20 do
        started = System.monotonic_time(:microsecond)
        count = Reconciler.tick()
        {System.monotonic_time(:microsecond) - started, count}
      end

    {tick_samples, counts} = Enum.unzip(ticks)
    account = member(fixture)

    resolves =
      for _ <- 1..2_000 do
        channel_id = Enum.random(fixture.channel_ids)
        started = System.monotonic_time(:microsecond)
        {:ok, _} = Access.channel(account.id, channel_id)
        System.monotonic_time(:microsecond) - started
      end

    info(
      "\nreconciliation: a tick with #{Enum.max(counts)} heads moved in the window, and one re-resolution"
    )

    tick = report("reconciler tick (#{Enum.max(counts)} channels)", tick_samples)
    resolve = report("re-resolution of one subscription (Access.channel)", resolves)
    %{tick: tick, resolve: resolve, heads: Enum.max(counts)}
  end

  # ---- Connections ----------------------------------------------------------------

  # N sessions on this node, 50 subscriptions each over the fixture's
  # channels at their heads, a tenth watching the server's presence, then
  # one message a second per thousand connections to random channels. The
  # clients share the VM, so the per-connection memory subtracts their own
  # processes; the sockets' kernel buffers on both ends stay in the RSS.
  # `ramp` connections a second: a subscribe to 50 channels is 150 queries
  # today (one access, head and page per channel), and a ramp past what
  # the pool serves makes DBConnection drop queued queries, which crashes
  # the session mid-subscribe; the spike records the rate at which that
  # happens.
  defp connections(fixture, body, n, server_pid, ramp) do
    coordinator = self()
    channel_ids = fixture.channel_ids
    account = member(fixture)
    same_vm? = server_pid == System.pid()

    info(
      "\nconnections: #{n} sessions × #{@resume_channels} subscriptions, #{div(n, 10)} in presence, #{ramp}/s" <>
        if(same_vm?, do: ", clients in the server's VM", else: ", server pid #{server_pid}")
    )

    rss_before = rss_kb(server_pid)
    started = System.monotonic_time(:millisecond)

    heads = heads(fixture)

    clients =
      1..n
      |> Enum.chunk_every(max(div(ramp, 4), 1))
      |> Enum.flat_map(fn batch ->
        started = Enum.map(batch, &connection(&1, fixture, heads, token(account), coordinator))
        Process.sleep(250)
        started
      end)

    wait_for(:subscribed, n, 600_000)
    seconds = (System.monotonic_time(:millisecond) - started) / 1_000
    info("  all #{n} connected and subscribed in #{Float.round(seconds, 1)} s")
    Process.sleep(5_000)
    rss_after = rss_kb(server_pid)
    server_bytes = div((rss_after - rss_before) * 1_024, n)

    info(
      "  server RSS #{div(rss_before, 1_024)} → #{div(rss_after, 1_024)} MB: #{div(server_bytes, 1_024)} KB per connection" <>
        if(same_vm?, do: " (the clients' own memory included)", else: "")
    )

    # subscribers per channel, for the expected delivery count
    by_channel =
      Enum.reduce(clients, %{}, fn {_pid, subs}, acc ->
        Enum.reduce(subs, acc, fn {id, _}, acc -> Map.update(acc, id, 1, &(&1 + 1)) end)
      end)

    sender = start_client(token(account), coordinator, subscribe: [])
    wait_for(:subscribed, 1)
    per_second = max(div(n, 1_000), 1)
    seconds = 20
    info("  sending #{per_second} messages/s for #{seconds} s")

    sent =
      for _ <- 1..(per_second * seconds), into: %{} do
        channel_id = Enum.random(channel_ids)
        command = command(channel_id, body)
        sent_at = System.monotonic_time(:microsecond)
        send(sender, {:send, command})
        Process.sleep(div(1_000, per_second))
        {command.command_id, {sent_at, Map.get(by_channel, channel_id, 0)}}
      end

    expected = sent |> Map.values() |> Enum.map(&elem(&1, 1)) |> Enum.sum()
    samples = collect_deliveries(expected, Map.new(sent, fn {k, {at, _}} -> {k, at} end), [])
    _ = collect_acks(map_size(sent), Map.new(sent, fn {k, {at, _}} -> {k, at} end), [])

    delivered =
      report("send to delivered under #{n} connections (#{expected} deliveries)", samples)

    Enum.each(clients, fn {pid, _} -> send(pid, :stop) end)
    send(sender, :stop)
    %{count: n, server_bytes: server_bytes, delivered: delivered}
  end

  # The i-th connection: 50 random channels at their heads, every tenth
  # watching the server's presence.
  defp connection(i, fixture, heads, token, coordinator) do
    subs =
      fixture.channel_ids
      |> Enum.take_random(@resume_channels)
      |> Enum.map(&{&1, heads[&1]})

    presence = if rem(i, 10) == 0, do: fixture.server_id
    {start_client(token, coordinator, subscribe: subs, presence: presence), subs}
  end

  defp rss_kb(pid) do
    {out, 0} = System.cmd("ps", ["-o", "rss=", "-p", to_string(pid)])
    out |> String.trim() |> String.to_integer()
  end

  defp mb(bytes), do: div(bytes, 1_048_576)

  # ---- Client ---------------------------------------------------------------------

  # One real WebSocket client. On Welcome it subscribes to `subscribe`
  # (cursors) and, with `presence`, watches that server; it reports
  # `:subscribed`, deliveries and acks to the coordinator, and with
  # `time_from_connect` the elapsed time from before the TCP connect.
  defp start_client(token, coordinator, script) do
    spawn_link(fn -> client(token, coordinator, Map.new(script)) end)
  end

  defp client(token, coordinator, script) do
    started = System.monotonic_time(:microsecond)
    conn = connect(5)
    # The token travels as a subprotocol (protocol section 6.1), the way
    # every client sends it.
    protocols = "phoenix, base64url.bearer.phx." <> Base.encode64(token, padding: false)
    headers = [{"sec-websocket-protocol", protocols}]
    {:ok, conn, ref} = Mint.WebSocket.upgrade(:ws, conn, "/gateway/websocket?vsn=1.0.0", headers)
    {conn, websocket} = await_upgrade(conn, ref)

    state = %{
      conn: conn,
      ref: ref,
      ws: websocket,
      coordinator: coordinator,
      script: script,
      started: started,
      presence_reported: false
    }

    state =
      send_frame(
        state,
        {:hello, %V1.Hello{client: "load", device_id: Proto.uuid(Ecto.UUID.generate())}}
      )

    # The transport closes a session after two missed heartbeats; a
    # connection that lives through a long run has to send them.
    Process.send_after(self(), :heartbeat, @heartbeat_ms)
    loop(state)
  end

  # macOS caps the listen backlog at 128 (kern.ipc.somaxconn), so a burst of
  # connects gets reset; retry with a short backoff rather than fail the run.
  defp connect(attempts) do
    case Mint.HTTP.connect(:http, "127.0.0.1", @port, protocols: [:http1]) do
      {:ok, conn} ->
        conn

      {:error, _} when attempts > 1 ->
        Process.sleep(50 + :rand.uniform(100)) && connect(attempts - 1)

      {:error, reason} ->
        exit({:connect_failed, reason})
    end
  end

  defp await_upgrade(conn, ref) do
    receive do
      message ->
        case Mint.WebSocket.stream(conn, message) do
          {:ok, conn, [{:status, ^ref, status}, {:headers, ^ref, headers} | _]} ->
            {:ok, conn, websocket} = Mint.WebSocket.new(conn, ref, status, headers)
            {conn, websocket}

          {:ok, conn, _} ->
            await_upgrade(conn, ref)
        end
    end
  end

  defp loop(state) do
    receive do
      :stop ->
        Mint.HTTP.close(state.conn)
        :ok

      {:send, command} ->
        state |> send_frame({:command, command}) |> loop()

      :heartbeat ->
        Process.send_after(self(), :heartbeat, @heartbeat_ms)
        state |> send_frame({:heartbeat, %V1.Heartbeat{}}) |> loop()

      message ->
        case Mint.WebSocket.stream(state.conn, message) do
          {:ok, conn, responses} ->
            state = %{state | conn: conn}
            state = Enum.reduce(responses, state, &handle_response/2)
            loop(state)

          {:error, _conn, reason, _} ->
            exit({:client_error, reason})
        end
    end
  end

  defp handle_response({:data, ref, data}, %{ref: ref} = state) do
    {:ok, ws, frames} = Mint.WebSocket.decode(state.ws, data)
    state = %{state | ws: ws}
    Enum.reduce(frames, state, &handle_frame/2)
  end

  defp handle_response(_other, state), do: state

  defp handle_frame({:binary, bytes}, state) do
    now = System.monotonic_time(:microsecond)

    case V1.ServerFrame.decode(bytes) do
      %V1.ServerFrame{frame: {tag, body}} -> on_frame(tag, body, now, state)
      _ -> state
    end
  end

  defp handle_frame({:close, code, reason}, _state), do: exit({:closed, code, reason})
  defp handle_frame(_frame, state), do: state

  defp on_frame(:welcome, _welcome, _now, state) do
    cursors =
      for {channel_id, seq} <- state.script.subscribe,
          do: %V1.Cursor{log_id: Proto.uuid(channel_id), seq: seq}

    state = send_frame(state, {:subscribe, %V1.Subscribe{cursors: cursors}})

    case state.script[:presence] do
      nil ->
        state

      server_id ->
        update = %V1.PresenceUpdate{status: :ONLINE, watch_server_ids: [Proto.uuid(server_id)]}
        send_frame(state, {:presence, update})
    end
  end

  defp on_frame(:subscribe_result, result, now, state) do
    Enum.each(result.results, fn r ->
      r.status == :RESUMED || exit({:subscribe_failed, r.status, r.error})
    end)

    if state.script[:time_from_connect],
      do: send(state.coordinator, {{:resumed_in, self()}, now - state.started})

    send(state.coordinator, :subscribed)
    state
  end

  defp on_frame(:presence_diff, _diff, now, state) do
    if state.script[:time_from_connect] == true and not state.presence_reported,
      do: send(state.coordinator, {{:presence_in, self()}, now - state.started})

    %{state | presence_reported: true}
  end

  defp on_frame(:event, %V1.Event{command_id: command_id}, now, state)
       when byte_size(command_id) == 16 do
    send(state.coordinator, {:delivered, command_id, now})
    state
  end

  defp on_frame(:ack, %V1.Ack{command_id: command_id}, now, state) do
    send(state.coordinator, {:acked, command_id, now})
    state
  end

  defp on_frame(:error, error, _now, _state), do: exit({:server_error, error})

  defp on_frame(:disconnect, %V1.Disconnect{reason: reason}, _now, state) do
    exit({:disconnected, reason, Map.keys(state.script), state.presence_reported})
  end

  defp on_frame(_tag, _body, _now, state), do: state

  defp send_frame(state, {tag, body}) do
    bytes = V1.ClientFrame.encode(%V1.ClientFrame{frame: {tag, body}})
    {:ok, ws, data} = Mint.WebSocket.encode(state.ws, {:binary, bytes})
    {:ok, conn} = Mint.WebSocket.stream_request_body(state.conn, state.ref, data)
    %{state | ws: ws, conn: conn}
  end

  # ---- Coordinator ----------------------------------------------------------------

  defp wait_for(tag, count \\ 1, timeout \\ 60_000)

  defp wait_for(_tag, 0, _timeout), do: :ok

  defp wait_for(tag, count, timeout) when is_atom(tag) do
    receive do
      ^tag -> wait_for(tag, count - 1, timeout)
    after
      timeout -> Mix.raise("timed out waiting for #{count} × #{inspect(tag)}")
    end
  end

  defp wait_for(tag, 1, timeout) do
    receive do
      {^tag, value} -> value
    after
      timeout -> Mix.raise("timed out waiting for #{inspect(tag)}")
    end
  end

  defp collect_deliveries(0, _sent, acc), do: acc

  defp collect_deliveries(n, sent, acc) do
    receive do
      {:delivered, command_id, at} ->
        case Map.fetch(sent, command_id) do
          {:ok, sent_at} -> collect_deliveries(n - 1, sent, [at - sent_at | acc])
          # a background write delivered to nobody we track
          :error -> collect_deliveries(n, sent, acc)
        end

      {:acked, _, _} = ack ->
        send(self(), ack)
        collect_deliveries(n, sent, acc)
    after
      15_000 ->
        Mix.raise("timed out waiting for deliveries; #{n} outstanding of #{length(acc) + n}")
    end
  end

  defp collect_acks(0, _sent, acc), do: acc

  defp collect_acks(n, sent, acc) do
    receive do
      {:acked, command_id, at} ->
        collect_acks(n - 1, sent, [at - Map.fetch!(sent, command_id) | acc])

      {:delivered, _, _} ->
        collect_acks(n, sent, acc)
    after
      5_000 -> Mix.raise("timed out waiting for acks; #{n} outstanding")
    end
  end

  # ---- Report ---------------------------------------------------------------------

  defp report(label, samples) do
    sorted = Enum.sort(samples)
    n = length(sorted)
    pct = fn p -> Enum.at(sorted, min(n - 1, trunc(p * n))) / 1_000 end

    stats = %{
      n: n,
      p50: pct.(0.50),
      p95: pct.(0.95),
      p99: pct.(0.99),
      max: List.last(sorted) / 1_000
    }

    info("""

    #{label}
      samples  #{n}
      p50      #{Float.round(stats.p50, 2)} ms
      p95      #{Float.round(stats.p95, 2)} ms
      p99      #{Float.round(stats.p99, 2)} ms
      max      #{Float.round(stats.max, 2)} ms
    """)

    stats
  end

  defp summary(results) do
    rows = [
      {"send to delivered, p99", ms(results.send.p99), "#{@budgets[:send_p99_ms]} ms"},
      {"connect + resume 50 channels, p99", ms(results.resume.p99),
       "#{@budgets[:resume_p99_ms]} ms"},
      {"scrollback of 50 messages, p99", ms(results.scrollback.p99),
       "#{@budgets[:scrollback_p99_ms]} ms"},
      {"frame overhead", "#{results.frame.overhead} bytes",
       "#{@budgets[:frame_overhead_bytes]} bytes"},
      {"reconciler tick, #{results.reconcile.heads} heads, p50", ms(results.reconcile.tick.p50),
       "one tick"},
      {"re-resolution, p99", ms(results.reconcile.resolve.p99), "per stale delivery"}
    ]

    rows =
      case results[:connections] do
        nil ->
          rows

        c ->
          rows ++
            [
              {"#{c.count} connections: RSS each, clients included",
               "#{div(c.server_bytes, 1_024)} KB", "100k in 8 GB"},
              {"#{c.count} connections: delivery p99", ms(c.delivered.p99),
               "#{@budgets[:send_p99_ms]} ms"}
            ]
      end

    width = rows |> Enum.map(&String.length(elem(&1, 0))) |> Enum.max()

    lines =
      Enum.map_join(rows, "\n", fn {label, value, budget} ->
        "  #{String.pad_trailing(label, width)}  #{String.pad_leading(value, 12)}  (budget #{budget})"
      end)

    info("\nsummary\n" <> lines <> "\n")
  end

  defp ms(value), do: "#{Float.round(value, 2)} ms"

  defp check_budgets(results) do
    failures =
      [
        {"send to delivered p99", results.send.p99, @budgets[:send_p99_ms]},
        {"connect + resume p99", results.resume.p99, @budgets[:resume_p99_ms]},
        {"scrollback p99", results.scrollback.p99, @budgets[:scrollback_p99_ms]},
        {"frame overhead", results.frame.overhead, @budgets[:frame_overhead_bytes]}
      ]
      |> Enum.reject(fn {_, value, budget} -> value <= budget end)

    case failures do
      [] ->
        info("budgets: ok")

      failures ->
        Enum.each(failures, fn {label, value, budget} ->
          info("budget missed: #{label} = #{value}, budget #{budget}")
        end)

        exit({:shutdown, 1})
    end
  end

  defp info(text), do: Mix.shell().info(text)
end
