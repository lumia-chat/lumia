defmodule LumiaWeb.Plugs.Cors do
  @moduledoc """
  Lets the origins this instance trusts call the API from a browser: its
  own origin and the configured flagship origins, the same list a web
  device's certificate is checked against (architecture 11.4, identity
  §7). A preflight from any other origin is refused; a plain request from
  one is answered without the headers, so the browser withholds the
  response and a non-browser caller is unaffected. `socket_origin?/1` is
  the same rule for the gateway's `check_origin`.
  """

  import Plug.Conn

  @methods "GET, POST, PUT, PATCH, DELETE, OPTIONS"
  # `x-reauth-token` carries a reauth to `POST /identity/link` (identity
  # spec 10); without it a browser on the flagship's origin could not bind
  # an account on another instance.
  @headers "authorization, content-type, accept, x-reauth-token"

  @doc "The origins that may call the API and open the gateway from a browser."
  def allowed_origins, do: Lumia.Identity.web_origins()

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

  # Every answer varies by the request's origin, one without an origin too: a
  # cache that kept a plain `<img>`'s answer would otherwise hand it to a
  # cross-origin `fetch` of the same picture, which then fails for want of
  # the CORS headers.
  def call(conn, _opts) do
    conn = put_resp_header(conn, "vary", "origin")

    case get_req_header(conn, "origin") do
      [origin] -> respond(conn, origin, origin in allowed_origins())
      _ -> conn
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
