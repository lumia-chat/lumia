defmodule Mix.Tasks.Lumia.Spike.Gateway do
  @shortdoc "Measures send-to-delivered latency through the real gateway"

  @moduledoc """
  Starts the endpoint, connects N real WebSocket clients to one channel,
  sends M messages from one of them, and measures the time from the
  sender's command to every subscriber's receipt of the event frame. This
  exercises Bandit, the serializer, the session channel, PubSub fan-out,
  and the write path, which is the "send to delivered" row in
  architecture section 17.

      mix lumia.spike.gateway [--subscribers 200] [--messages 300] [--rate 50]

  Runs against the dev database and leaves rows behind; `mix ecto.reset`
  clears them.
  """

  use Mix.Task

  alias Lumia.Accounts
  alias Lumia.Auth
  alias Lumia.Proto
  alias Lumia.Servers
  alias Lumia.V1

  @switches [subscribers: :integer, messages: :integer, rate: :integer]
  @port 4799

  @impl Mix.Task
  def run(args) do
    {opts, _} = OptionParser.parse!(args, strict: @switches)
    subscribers = Keyword.get(opts, :subscribers, 200)
    messages = Keyword.get(opts, :messages, 300)
    rate = Keyword.get(opts, :rate, 50)

    Logger.configure(level: :warning)
    # Load the config files first, then override: app.start would otherwise
    # reload them over the override.
    Mix.Task.run("app.config")

    Application.put_env(
      :lumia,
      LumiaWeb.Endpoint,
      Keyword.merge(Application.get_env(:lumia, LumiaWeb.Endpoint),
        server: true,
        http: [ip: {127, 0, 0, 1}, port: @port]
      )
    )

    Mix.Task.run("app.start")

    Mix.shell().info("gateway spike: subscribers=#{subscribers} messages=#{messages} rate=#{rate}/s")

    {channel_id, tokens} = setup(subscribers + 1)
    [sender_token | subscriber_tokens] = tokens

    coordinator = self()
    clients =
      subscriber_tokens
      |> Enum.chunk_every(50)
      |> Enum.flat_map(fn batch ->
        started = for token <- batch, do: start_client(token, channel_id, coordinator)
        Process.sleep(100)
        started
      end)
    sender = start_client(sender_token, channel_id, coordinator)
    wait_ready(length(clients) + 1)
    Mix.shell().info("all #{length(clients) + 1} clients subscribed")

    sent =
      for i <- 1..messages, into: %{} do
        command = command(channel_id, "message #{i}")
        sent_at = System.monotonic_time(:microsecond)
        send(sender, {:send, command})
        Process.sleep(div(1_000, rate))
        {command.command_id, sent_at}
      end

    samples = collect(map_size(sent) * length(clients), sent, [])
    acks = collect_acks(messages, sent, [])

    report("send to delivered (#{length(clients)} subscribers × #{messages} messages)", samples)
    report("send to ack", acks)
  end

  # ---- Setup -------------------------------------------------------------------------

  defp setup(n) do
    {:ok, owner} = Accounts.create_account(%{handle: "spike-owner-#{System.unique_integer([:positive])}"})
    {:ok, server} = Servers.create_server(owner.id, "Spike")
    {:ok, channel} = Servers.create_channel(server.id, owner.id, "general")

    tokens =
      for i <- 1..n do
        {:ok, account} = Accounts.create_account(%{handle: "spike-#{i}-#{System.unique_integer([:positive])}"})
        {:ok, _} = Servers.add_member(server.id, account.id, owner.id)
        {:ok, _, tokens} = Auth.create_session(account.id, Ecto.UUID.generate())
        tokens.access_token
      end

    {channel.id, tokens}
  end

  defp command(channel_id, body) do
    %V1.Command{
      command_id: Proto.uuid(Ecto.UUID.generate(version: 7)),
      channel_id: Proto.uuid(channel_id),
      op: {:send_message, %V1.SendMessage{body: body}}
    }
  end

  # ---- Client -----------------------------------------------------------------------

  defp start_client(token, channel_id, coordinator) do
    spawn_link(fn -> client(token, channel_id, coordinator) end)
  end

  defp client(token, channel_id, coordinator) do
    conn = connect(5)
    path = "/gateway/websocket?access_token=#{token}&vsn=1.0.0"
    {:ok, conn, ref} = Mint.WebSocket.upgrade(:ws, conn, path, [])

    {conn, websocket} = await_upgrade(conn, ref)
    state = %{conn: conn, ref: ref, ws: websocket, coordinator: coordinator, channel_id: channel_id}

    state = send_frame(state, {:hello, %V1.Hello{client: "spike", device_id: Proto.uuid(Ecto.UUID.generate())}})
    loop(state)
  end

  # macOS's default listen backlog is 128, so a burst of connects gets
  # reset; retry with a short backoff rather than fail the run.
  defp connect(attempts) do
    case Mint.HTTP.connect(:http, "127.0.0.1", @port, protocols: [:http1]) do
      {:ok, conn} -> conn
      {:error, _} when attempts > 1 -> Process.sleep(50 + :rand.uniform(100)) && connect(attempts - 1)
      {:error, reason} -> exit({:connect_failed, reason})
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
      {:send, command} ->
        state |> send_frame({:command, command}) |> loop()

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
      %V1.ServerFrame{frame: {:welcome, _}} ->
        cursor = %V1.Cursor{log_id: Proto.uuid(state.channel_id), seq: 0}
        send_frame(state, {:subscribe, %V1.Subscribe{cursors: [cursor]}})

      %V1.ServerFrame{frame: {:subscribe_result, _}} ->
        send(state.coordinator, :ready)
        state

      %V1.ServerFrame{frame: {:event, %V1.Event{command_id: command_id}}} when byte_size(command_id) == 16 ->
        send(state.coordinator, {:delivered, command_id, now})
        state

      %V1.ServerFrame{frame: {:ack, %V1.Ack{command_id: command_id}}} ->
        send(state.coordinator, {:acked, command_id, now})
        state

      _ ->
        state
    end
  end

  defp handle_frame(_frame, state), do: state

  defp send_frame(state, {tag, body}) do
    bytes = V1.ClientFrame.encode(%V1.ClientFrame{frame: {tag, body}})
    {:ok, ws, data} = Mint.WebSocket.encode(state.ws, {:binary, bytes})
    {:ok, conn} = Mint.WebSocket.stream_request_body(state.conn, state.ref, data)
    %{state | ws: ws, conn: conn}
  end

  # ---- Coordinator ------------------------------------------------------------------

  defp wait_ready(0), do: :ok

  defp wait_ready(n) do
    receive do
      :ready -> wait_ready(n - 1)
    after
      30_000 -> Mix.raise("clients did not subscribe in time; #{n} outstanding")
    end
  end

  defp collect(0, _sent, acc), do: acc

  defp collect(n, sent, acc) do
    receive do
      {:delivered, command_id, at} -> collect(n - 1, sent, [at - Map.fetch!(sent, command_id) | acc])
      {:acked, _, _} = ack -> send(self(), ack) |> then(fn _ -> collect(n, sent, acc) end)
    after
      10_000 -> Mix.raise("timed out waiting for deliveries; #{n} outstanding of #{length(acc) + n}")
    end
  end

  defp collect_acks(0, _sent, acc), do: acc

  defp collect_acks(n, sent, acc) do
    receive do
      {:acked, command_id, at} -> collect_acks(n - 1, sent, [at - Map.fetch!(sent, command_id) | acc])
    after
      5_000 -> Mix.raise("timed out waiting for acks; #{n} outstanding")
    end
  end

  defp report(label, samples) do
    sorted = Enum.sort(samples)
    n = length(sorted)
    pct = fn p -> Enum.at(sorted, min(n - 1, trunc(p * n))) / 1_000 end

    Mix.shell().info("""

    #{label}
      samples  #{n}
      p50      #{Float.round(pct.(0.50), 2)} ms
      p95      #{Float.round(pct.(0.95), 2)} ms
      p99      #{Float.round(pct.(0.99), 2)} ms
      max      #{Float.round(List.last(sorted) / 1_000, 2)} ms
    """)
  end
end
