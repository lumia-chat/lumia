defmodule LumiaWeb.Gateway.Socket do
  @moduledoc """
  The gateway WebSocket (docs/protocol.md section 6). Authenticated by the
  access token on the upgrade request; one `session` channel per socket.

  The token arrives as the `access_token` query parameter. Phoenix sockets
  cannot read arbitrary request headers, so the `Authorization` header form
  in the protocol doc waits for a custom upgrade plug.
  """

  use Phoenix.Socket

  alias Lumia.Auth

  channel "session", LumiaWeb.Gateway.SessionChannel

  @impl Phoenix.Socket
  def connect(params, socket, _connect_info) do
    case Auth.verify_access(params["access_token"]) do
      {:ok, session} ->
        {:ok,
         assign(socket,
           session_id: session.id,
           account_id: session.account_id,
           device_id: session.device_id
         )}

      {:error, _} ->
        :error
    end
  end

  # One socket per session; a second connection with the same session
  # disconnects the first (protocol 6.1).
  @impl Phoenix.Socket
  def id(socket), do: "gateway:" <> socket.assigns.session_id
end
