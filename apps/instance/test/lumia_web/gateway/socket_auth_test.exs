defmodule LumiaWeb.Gateway.SocketAuthTest do
  @moduledoc "The access token on the upgrade (protocol section 6.1): query or subprotocol."

  use LumiaWeb.ChannelCase, async: true

  test "the access token is read from the query or from the Sec-WebSocket-Protocol token" do
    account = Lumia.Fixtures.account("sock")
    {:ok, _session, tokens} = Lumia.Auth.create_session(account.id, Ecto.UUID.generate())

    assert {:ok, by_query} =
             connect(LumiaWeb.Gateway.Socket, %{"access_token" => tokens.access_token})

    assert by_query.assigns.account_id == account.id

    assert {:ok, by_header} =
             connect(LumiaWeb.Gateway.Socket, %{},
               connect_info: %{auth_token: tokens.access_token}
             )

    assert by_header.assigns.account_id == account.id
    assert :error = connect(LumiaWeb.Gateway.Socket, %{})
  end
end
