defmodule LumiaWeb.Telemetry do
  use Supervisor
  import Telemetry.Metrics

  def start_link(arg) do
    Supervisor.start_link(__MODULE__, arg, name: __MODULE__)
  end

  @impl true
  def init(_arg) do
    children = [
      # Telemetry poller will execute the given period measurements
      # every 10_000ms. Learn more here: https://telemetry-metrics.hexdocs.pm
      {:telemetry_poller, measurements: periodic_measurements(), period: 10_000},
      # The Prometheus text served at /metrics (docs/specs/deployment.md
      # section 7) when LUMIA_METRICS is on; LumiaWeb.Plugs.Metrics scrapes it.
      {TelemetryMetricsPrometheus.Core, metrics: prometheus_metrics()},
      # Folds the pending distribution samples into their buckets every 30
      # seconds, so the sample table stays bounded when nothing scrapes.
      LumiaWeb.Telemetry.Aggregator
    ]

    Supervisor.init(children, strategy: :one_for_one)
  end

  @doc """
  What `/metrics` exports: the request and query durations as
  distributions, the VM's memory and run queues, and the instance's own
  counters (gateway connections, sessions joined, commands acked,
  reconciler ticks and the heads they found behind).
  """
  def prometheus_metrics do
    buckets = [1, 5, 10, 25, 50, 100, 250, 500, 1_000, 2_500]

    [
      distribution("phoenix.endpoint.stop.duration",
        unit: {:native, :millisecond},
        reporter_options: [buckets: buckets]
      ),
      distribution("phoenix.router_dispatch.stop.duration",
        tags: [:route],
        unit: {:native, :millisecond},
        reporter_options: [buckets: buckets]
      ),
      distribution("lumia.repo.query.total_time",
        unit: {:native, :millisecond},
        reporter_options: [buckets: buckets]
      ),
      last_value("vm.memory.total", unit: {:byte, :kilobyte}),
      last_value("vm.total_run_queue_lengths.total"),
      counter("lumia.gateway.connect.count"),
      counter("lumia.gateway.session_joined.count"),
      counter("lumia.gateway.ack.count"),
      counter("lumia.reconciler.tick.count"),
      sum("lumia.reconciler.tick.behind")
    ]
  end

  def metrics do
    [
      # Phoenix Metrics
      summary("phoenix.endpoint.start.system_time",
        unit: {:native, :millisecond}
      ),
      summary("phoenix.endpoint.stop.duration",
        unit: {:native, :millisecond}
      ),
      summary("phoenix.router_dispatch.start.system_time",
        tags: [:route],
        unit: {:native, :millisecond}
      ),
      summary("phoenix.router_dispatch.exception.duration",
        tags: [:route],
        unit: {:native, :millisecond}
      ),
      summary("phoenix.router_dispatch.stop.duration",
        tags: [:route],
        unit: {:native, :millisecond}
      ),
      summary("phoenix.socket_connected.duration",
        unit: {:native, :millisecond}
      ),
      sum("phoenix.socket_drain.count"),
      summary("phoenix.channel_joined.duration",
        unit: {:native, :millisecond}
      ),
      summary("phoenix.channel_handled_in.duration",
        tags: [:event],
        unit: {:native, :millisecond}
      ),

      # Database Metrics
      summary("lumia.repo.query.total_time",
        unit: {:native, :millisecond},
        description: "The sum of the other measurements"
      ),
      summary("lumia.repo.query.decode_time",
        unit: {:native, :millisecond},
        description: "The time spent decoding the data received from the database"
      ),
      summary("lumia.repo.query.query_time",
        unit: {:native, :millisecond},
        description: "The time spent executing the query"
      ),
      summary("lumia.repo.query.queue_time",
        unit: {:native, :millisecond},
        description: "The time spent waiting for a database connection"
      ),
      summary("lumia.repo.query.idle_time",
        unit: {:native, :millisecond},
        description:
          "The time the connection spent waiting before being checked out for the query"
      ),

      # VM Metrics
      summary("vm.memory.total", unit: {:byte, :kilobyte}),
      summary("vm.total_run_queue_lengths.total"),
      summary("vm.total_run_queue_lengths.cpu"),
      summary("vm.total_run_queue_lengths.io")
    ]
  end

  defp periodic_measurements do
    [
      # A module, function and arguments to be invoked periodically.
      # This function must call :telemetry.execute/3 and a metric must be added above.
      # {LumiaWeb, :count_users, []}
    ]
  end
end
