defmodule Mix.Tasks.Lumia.Spike.Events do
  @shortdoc "Measures the event-log write path and checks partition pruning"

  @moduledoc """
  Hammers `Lumia.Events.append/2` from concurrent writers and reports
  throughput and latency percentiles, then runs EXPLAIN on the sync,
  idempotency and reconciliation queries and checks that each touches one
  partition.

      mix lumia.spike.events [--channels 1,100] [--writers 50] [--duration 20] [--payload-bytes 1024]

  Each entry in `--channels` is a scenario: 1 is the hot-channel case where
  every write serializes on one row; 100 spreads writers across channels.
  Runs against the dev database and leaves its rows behind; `mix ecto.reset`
  clears them.
  """

  use Mix.Task

  alias Lumia.Events
  alias Lumia.Repo

  @switches [channels: :string, writers: :integer, duration: :integer, payload_bytes: :integer]
  @actor "00000000-0000-7000-8000-000000000001"

  @impl Mix.Task
  def run(args) do
    {opts, _} = OptionParser.parse!(args, strict: @switches)
    # The dev config logs every query; that is noise at thousands of writes a second.
    Logger.configure(level: :warning)
    Mix.Task.run("app.start")

    channel_counts =
      opts
      |> Keyword.get(:channels, "1,100")
      |> String.split(",")
      |> Enum.map(&String.to_integer/1)

    writers = Keyword.get(opts, :writers, 50)
    duration = Keyword.get(opts, :duration, 20)
    payload_bytes = Keyword.get(opts, :payload_bytes, 1024)

    Mix.shell().info(
      "event-log spike: writers=#{writers} duration=#{duration}s payload=#{payload_bytes}B scenarios=#{inspect(channel_counts)}"
    )

    body = :crypto.strong_rand_bytes(div(payload_bytes, 2)) |> Base.encode16(case: :lower)

    channel_sets =
      Map.new(channel_counts, fn n -> {n, for(_ <- 1..n, do: create_channel!())} end)

    scenarios =
      Map.new(channel_sets, fn {n, channel_ids} ->
        {"#{n} channel(s)", fn -> append!(Enum.random(channel_ids), body) end}
      end)

    suite =
      Benchee.run(scenarios,
        parallel: writers,
        time: duration,
        warmup: 2,
        percentiles: [50, 95, 99],
        formatters: [],
        print: [benchmarking: false, configuration: false, fast_warning: false]
      )

    for scenario <- suite.scenarios do
      stats = scenario.run_time_data.statistics
      total = stats.sample_size
      ops_per_sec = total / duration

      Mix.shell().info("""

      #{scenario.name}
        writes      #{total}
        throughput  #{Float.round(ops_per_sec, 1)} writes/s across #{writers} writers
        p50         #{ms(stats.percentiles[50])} ms
        p95         #{ms(stats.percentiles[95])} ms
        p99         #{ms(stats.percentiles[99])} ms
        max         #{ms(stats.maximum)} ms
      """)
    end

    # Explain against a channel that has rows in it.
    explain_all(channel_sets |> Map.values() |> hd() |> hd())
  end

  defp create_channel! do
    {:ok, channel} = Events.create_channel()
    channel.id
  end

  defp append!(channel_id, body) do
    command = %Lumia.V1.Command{
      command_id: Lumia.Proto.uuid(Ecto.UUID.generate(version: 7)),
      channel_id: Lumia.Proto.uuid(channel_id),
      op: {:send_message, %Lumia.V1.SendMessage{body: body}}
    }

    {:ok, _ack} =
      Events.append(channel_id, %{account_id: @actor, device_id: nil}, command, broadcast: false)
  end

  defp explain_all(channel_id) do
    {:ok, bin} = Ecto.UUID.dump(channel_id)
    cmd = Ecto.UUID.generate(version: 7)
    {:ok, cmd_bin} = Ecto.UUID.dump(cmd)
    since = DateTime.utc_now() |> DateTime.add(-5, :second) |> DateTime.truncate(:second)

    explain(
      "sync: events after seq",
      "SELECT * FROM events WHERE channel_id = $1 AND seq > $2 ORDER BY seq LIMIT 200",
      [bin, 10],
      partitions_of: "events"
    )

    explain(
      "idempotency: command lookup",
      "SELECT * FROM commands WHERE channel_id = $1 AND command_id = $2",
      [bin, cmd_bin],
      partitions_of: "commands"
    )

    explain(
      "reconciliation: channels changed since",
      "SELECT id, last_seq FROM channels WHERE last_seq_at >= $1",
      [since],
      expect: ~r/Index|Bitmap/,
      table: "channels"
    )
  end

  defp small_table?(nil), do: false

  defp small_table?(table) do
    %{rows: [[count]]} = Repo.query!("SELECT count(*) FROM #{table}")
    count < 1_000
  end

  defp explain(label, sql, params, checks) do
    %{rows: rows} = Repo.query!("EXPLAIN (ANALYZE, FORMAT TEXT) " <> sql, params)
    plan = rows |> List.flatten() |> Enum.join("\n")

    verdict =
      cond do
        table = checks[:partitions_of] ->
          touched = Regex.scan(~r/#{table}_p\d+/, plan) |> List.flatten() |> Enum.uniq()

          if length(touched) == 1,
            do: "OK, one partition (#{hd(touched)})",
            else: "FAIL, touched #{inspect(touched)}"

        re = checks[:expect] ->
          cond do
            plan =~ re ->
              "OK, index used"

            # On a near-empty table the planner prefers a sequential scan,
            # which says nothing about the index. Only fail when it would
            # have mattered.
            small_table?(checks[:table]) ->
              "SKIPPED, table too small for the planner to use the index"

            true ->
              "FAIL, no index scan"
          end
      end

    Mix.shell().info("\n#{label}: #{verdict}\n" <> indent(plan))
  end

  defp indent(text), do: text |> String.split("\n") |> Enum.map_join("\n", &("    " <> &1))

  defp ms(nanoseconds), do: Float.round(nanoseconds / 1_000_000, 2)
end
