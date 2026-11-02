defmodule LumiaWeb.Gateway.SocketTest do
  @moduledoc """
  The transport-level rules of protocol section 6 that live in the
  serializer because Phoenix owns the socket process: a frame before Hello
  is a protocol error, answered with a Disconnect and then a close.
  """

  use ExUnit.Case, async: true

  alias Lumia.V1
  alias LumiaWeb.Gateway.Serializer
  alias Phoenix.Socket.Broadcast

  test "a frame before Hello queues the transport's disconnect" do
    frame = V1.ClientFrame.encode(%V1.ClientFrame{frame: {:heartbeat, %V1.Heartbeat{nonce: 1}}})

    assert %Phoenix.Socket.Message{topic: "session", event: "frame"} =
             Serializer.decode!(frame, opcode: :binary)

    assert_received %Broadcast{event: "disconnect"}
  end

  test "after Hello, frames do not" do
    hello = V1.ClientFrame.encode(%V1.ClientFrame{frame: {:hello, %V1.Hello{client: "t"}}})
    frame = V1.ClientFrame.encode(%V1.ClientFrame{frame: {:heartbeat, %V1.Heartbeat{nonce: 1}}})

    assert %Phoenix.Socket.Message{event: "phx_join"} = Serializer.decode!(hello, opcode: :binary)
    assert %Phoenix.Socket.Message{event: "frame"} = Serializer.decode!(frame, opcode: :binary)
    refute_received %Broadcast{event: "disconnect"}
  end

  test "Phoenix's own error reply for the unjoined topic is the Disconnect" do
    reply = %Phoenix.Socket.Reply{
      ref: nil,
      topic: "session",
      status: :error,
      payload: %{reason: "unmatched topic"}
    }

    assert {:socket_push, :binary, bytes} = Serializer.encode!(reply)

    assert %V1.ServerFrame{
             frame: {:disconnect, %V1.Disconnect{reason: :DISCONNECT_PROTOCOL_ERROR}}
           } = V1.ServerFrame.decode(IO.iodata_to_binary(bytes))
  end

  test "the access token is read from the query or from the Sec-WebSocket-Protocol token" do
    account = Lumia.Fixtures.account("sock")
    {:ok, _session, tokens} = Lumia.Auth.create_session(account.id, Ecto.UUID.generate())

    assert {:ok, by_query} =
             Phoenix.ChannelTest.connect(LumiaWeb.Gateway.Socket, %{
               "access_token" => tokens.access_token
             })

    assert by_query.assigns.account_id == account.id

    assert {:ok, by_header} =
             Phoenix.ChannelTest.connect(LumiaWeb.Gateway.Socket, %{},
               connect_info: %{auth_token: tokens.access_token}
             )

    assert by_header.assigns.account_id == account.id
    assert :error = Phoenix.ChannelTest.connect(LumiaWeb.Gateway.Socket, %{})
  end
end
