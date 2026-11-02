defmodule LumiaWeb.Plugs.MetricsTest do
  use LumiaWeb.ConnCase, async: false

  setup do
    on_exit(fn -> Application.put_env(:lumia, :metrics, false) end)
  end

  test "/metrics is off by default and serves Prometheus text when on", %{conn: conn} do
    Application.put_env(:lumia, :metrics, false)
    assert get(conn, "/metrics").status == 404

    Application.put_env(:lumia, :metrics, true)
    :telemetry.execute([:lumia, :reconciler, :tick], %{count: 1, behind: 3}, %{})
    conn = get(conn, "/metrics")
    assert conn.status == 200
    assert conn.resp_body =~ "lumia_reconciler_tick_count"
    assert conn.resp_body =~ "lumia_reconciler_tick_behind"
  end
end
