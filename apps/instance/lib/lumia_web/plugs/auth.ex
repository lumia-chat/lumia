defmodule LumiaWeb.Plugs.Auth do
  @moduledoc """
  Bearer authentication for the API (docs/protocol.md section 12). Verifies
  the access token against the session row on every request and assigns
  `current_session`, `current_account_id`, and `current_device_id`.
  """

  import Plug.Conn

  alias Lumia.Auth
  alias LumiaWeb.ProtoConn

  def init(opts), do: opts

  def call(conn, _opts) do
    with ["Bearer " <> token] <- get_req_header(conn, "authorization"),
         {:ok, session} <- Auth.verify_access(token) do
      conn
      |> assign(:current_session, session)
      |> assign(:current_account_id, session.account_id)
      |> assign(:current_device_id, session.device_id)
    else
      [] -> halt_with(conn, :unauthenticated)
      {:error, reason} -> halt_with(conn, reason)
      _ -> halt_with(conn, :unauthenticated)
    end
  end

  defp halt_with(conn, reason) do
    conn
    |> ProtoConn.render_error(reason)
    |> halt()
  end
end
