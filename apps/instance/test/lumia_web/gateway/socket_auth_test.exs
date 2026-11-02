defmodule LumiaWeb.Gateway.SocketAuthTest do
  @moduledoc "The access token on the upgrade (protocol section 6.1): the subprotocol only."

  use LumiaWeb.ChannelCase, async: true

  test "the access token is read from the Sec-WebSocket-Protocol token, never the query" do
    account = Lumia.Fixtures.account("sock")
    {:ok, _session, tokens} = Lumia.Auth.create_session(account.id, Ecto.UUID.generate())

    assert {:ok, by_header} =
             connect(LumiaWeb.Gateway.Socket, %{},
               connect_info: %{auth_token: tokens.access_token}
             )

    assert by_header.assigns.account_id == account.id

    assert :error = connect(LumiaWeb.Gateway.Socket, %{"access_token" => tokens.access_token})
    assert :error = connect(LumiaWeb.Gateway.Socket, %{})
  end
end
