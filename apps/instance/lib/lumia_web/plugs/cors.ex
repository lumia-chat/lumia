defmodule LumiaWeb.Plugs.Cors do
  @moduledoc """
  Lets the web origins this instance trusts call the API from a browser:
  its own origin and the configured flagship origins, the same list a
  web device's certificate is checked against (architecture 11.4,
  identity §7). A preflight from any other origin is refused; a plain
  request from one is answered without the headers, so the browser
  withholds the response and a non-browser caller is unaffected.
  """

  import Plug.Conn

  @methods "GET, POST, PUT, PATCH, DELETE, OPTIONS"
  @headers "authorization, content-type, accept"

  def init(opts), do: opts

  def call(conn, _opts) do
    case get_req_header(conn, "origin") do
      [origin] ->
        allowed? = origin in Lumia.Identity.web_origins()
        conn = put_resp_header(conn, "vary", "origin")
        respond(conn, origin, allowed?)

      _ ->
        conn
    end
  end

  defp respond(%{method: "OPTIONS"} = conn, origin, true) do
    conn
    |> allow(origin)
    |> put_resp_header("access-control-allow-methods", @methods)
    |> put_resp_header("access-control-allow-headers", @headers)
    |> put_resp_header("access-control-max-age", "86400")
    |> send_resp(204, "")
    |> halt()
  end

  defp respond(%{method: "OPTIONS"} = conn, _origin, false) do
    conn |> send_resp(403, "") |> halt()
  end

  defp respond(conn, origin, true), do: allow(conn, origin)
  defp respond(conn, _origin, false), do: conn

  defp allow(conn, origin), do: put_resp_header(conn, "access-control-allow-origin", origin)
end
