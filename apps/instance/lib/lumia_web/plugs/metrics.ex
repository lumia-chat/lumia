defmodule LumiaWeb.Plugs.Metrics do
  @moduledoc """
  Serves the Prometheus text at `/metrics` when `LUMIA_METRICS` is on
  (docs/specs/deployment.md section 7). Meant for the operator's network:
  the install guide keeps it off the public proxy.
  """

  import Plug.Conn

  def init(opts), do: opts

  def call(%Plug.Conn{request_path: "/metrics", method: "GET"} = conn, _opts) do
    if Application.get_env(:lumia, :metrics, false) do
      conn
      |> put_resp_content_type("text/plain", "utf-8")
      |> send_resp(200, TelemetryMetricsPrometheus.Core.scrape())
      |> halt()
    else
      conn
    end
  end

  def call(conn, _opts), do: conn
end
