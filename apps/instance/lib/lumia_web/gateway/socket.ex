defmodule LumiaWeb.Gateway.Socket do
  @moduledoc """
  The gateway WebSocket (docs/protocol.md section 6). Authenticated by the
  access token on the upgrade request; one `session` channel per socket.

  The token arrives as the `access_token` query parameter or, for browsers,
  which cannot set `Authorization` on a WebSocket, as the subprotocol
  `base64url.bearer.phx.<token>` in `Sec-WebSocket-Protocol` (protocol
  section 6). Phoenix sockets cannot read other request headers.
  """

  use Phoenix.Socket

  alias Lumia.Auth

  channel "session", LumiaWeb.Gateway.SessionChannel

  @impl Phoenix.Socket
  def connect(params, socket, connect_info) do
    token = params["access_token"] || connect_info[:auth_token]

    case Auth.verify_access(token) do
      {:ok, session} ->
        # One connection per session (protocol 6.1): a session process that
        # is already live for this session closes its socket with
        # DISCONNECT_PROTOCOL_ERROR. This connection's own session process
        # does not exist yet, so it cannot receive this.
        Phoenix.PubSub.broadcast(
          Lumia.PubSub,
          Auth.topic(session.id),
          {:session_superseded, session.id}
        )

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

  # Lets `LumiaWeb.Endpoint.broadcast(id, "disconnect", %{})` close every
  # socket of a session; the session process uses the same mechanism to
  # close its own transport.
  @impl Phoenix.Socket
  def id(socket), do: "gateway:" <> socket.assigns.session_id
end
