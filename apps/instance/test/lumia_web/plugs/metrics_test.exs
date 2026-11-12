defmodule LumiaWeb.Plugs.MetricsTest do
  use LumiaWeb.ConnCase, async: false

  setup do
    on_exit(fn -> Application.put_env(:lumia, :metrics, false) end)
  end

  test "/metrics is off by default and serves Prometheus text when on", %{conn: conn} do
    # Off, the path is nothing of the plug's: it falls to the router, whose
    # answer depends on whether a build of the web UI is in priv/static/ui
    # (its page, or a line saying there is none). Either way, not the metrics.
    Application.put_env(:lumia, :metrics, false)
    :telemetry.execute([:lumia, :reconciler, :tick], %{count: 1, behind: 3}, %{})
    refute get(conn, "/metrics").resp_body =~ "lumia_reconciler_tick"

    Application.put_env(:lumia, :metrics, true)
    :telemetry.execute([:lumia, :reconciler, :tick], %{count: 1, behind: 3}, %{})
    conn = get(conn, "/metrics")
    assert conn.status == 200
    assert conn.resp_body =~ "lumia_reconciler_tick_count"
    assert conn.resp_body =~ "lumia_reconciler_tick_behind"
  end
end
