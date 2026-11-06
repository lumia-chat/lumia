defmodule LumiaWeb.Telemetry.AggregatorTest do
  use ExUnit.Case, async: false

  alias LumiaWeb.Telemetry.Aggregator

  test "pending distribution samples are folded into buckets, not kept" do
    Aggregator.aggregate()

    for _ <- 1..100 do
      :telemetry.execute([:lumia, :repo, :query], %{total_time: 1_000}, %{})
    end

    assert :ets.info(:prometheus_metrics_dist, :size) >= 100
    assert :ok = Aggregator.aggregate()
    assert :ets.info(:prometheus_metrics_dist, :size) == 0
  end
end
