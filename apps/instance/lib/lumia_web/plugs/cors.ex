defmodule LumiaWeb.Plugs.Cors do
  @moduledoc """
  Lets the origins this instance trusts call the API from a webview: its
  own origin and the configured flagship origins, the same list a web
  device's certificate is checked against (architecture 11.4, identity
  §7), plus the desktop shell's (Tauri serves the bundled UI from a fixed
  local origin; the shell is a NATIVE device, so that origin never counts
  for a web-device certificate). A preflight from any other origin is
  refused; a plain request from one is answered without the headers, so
  the browser withholds the response and a non-browser caller is
  unaffected. `socket_origin?/1` is the same rule for the gateway's
  `check_origin`.
  """

  import Plug.Conn

  @methods "GET, POST, PUT, PATCH, DELETE, OPTIONS"
  @headers "authorization, content-type, accept"
  # Tauri 2: `tauri://localhost` on macOS and Linux, `https://tauri.localhost`
  # on Windows with `useHttpsScheme`, `http://tauri.localhost` without.
  @shell_origins ["tauri://localhost", "https://tauri.localhost", "http://tauri.localhost"]

  @doc "The origins that may call the API and open the gateway from a webview."
  def allowed_origins, do: Lumia.Identity.web_origins() ++ @shell_origins

  @doc "The gateway's `check_origin` rule: the same origins as the API's."
  def socket_origin?(%URI{} = uri) do
    origin =
      case uri do
        %URI{scheme: scheme, host: host, port: nil} -> "#{scheme}://#{host}"
        %URI{scheme: scheme, host: host, port: port} -> "#{scheme}://#{host}:#{port}"
      end

    origin in allowed_origins() or
      (uri.port in [80, 443] and "#{uri.scheme}://#{uri.host}" in allowed_origins())
  end

  def init(opts), do: opts

  def call(conn, _opts) do
    case get_req_header(conn, "origin") do
      [origin] ->
        allowed? = origin in allowed_origins()
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
