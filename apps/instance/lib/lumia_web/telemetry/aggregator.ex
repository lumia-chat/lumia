defmodule LumiaWeb.Telemetry.Aggregator do
  @moduledoc """
  Aggregates the Prometheus distributions on a timer.

  `TelemetryMetricsPrometheus.Core` keeps every distribution sample (one
  row per request and per repo query) in an ETS table until a scrape
  aggregates them into buckets, so on an instance nothing scrapes, or
  scrapes rarely, that table grows without bound: 300,000 rows after a
  thousand gateway sessions subscribed (docs/spikes/2026-11-06-load.md).
  Aggregating every 30 seconds bounds it to half a minute of samples
  whatever the operator runs; a scrape in between sees the same numbers.
  """

  use GenServer

  @interval_ms :timer.seconds(30)

  def start_link(opts), do: GenServer.start_link(__MODULE__, opts, name: __MODULE__)

  @doc "Aggregates the pending samples now, as a scrape would."
  def aggregate do
    _ = TelemetryMetricsPrometheus.Core.scrape()
    :ok
  end

  @impl GenServer
  def init(_opts) do
    schedule()
    {:ok, %{}}
  end

  @impl GenServer
  def handle_info(:aggregate, state) do
    aggregate()
    schedule()
    {:noreply, state}
  end

  defp schedule, do: Process.send_after(self(), :aggregate, @interval_ms)
end
