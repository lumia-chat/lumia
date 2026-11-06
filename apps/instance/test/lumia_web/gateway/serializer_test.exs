defmodule LumiaWeb.Gateway.SerializerTest do
  use ExUnit.Case, async: true

  alias Lumia.Proto
  alias Lumia.V1
  alias LumiaWeb.Gateway.Serializer
  alias Phoenix.Socket.Message
  alias Phoenix.Socket.Reply

  setup do
    Process.delete(:lumia_gateway_encoding)
    :ok
  end

  describe "decode!/2" do
    test "Hello becomes the join of topic session" do
      hello = %V1.Hello{client: "test", device_id: Proto.uuid(Ecto.UUID.generate())}
      raw = V1.ClientFrame.encode(%V1.ClientFrame{frame: {:hello, hello}})

      assert %Message{
               topic: "session",
               event: "phx_join",
               payload: ^hello,
               ref: "1",
               join_ref: "1"
             } =
               Serializer.decode!(raw, opcode: :binary)
    end

    test "any other frame is the frame event carrying the whole ClientFrame" do
      frame = %V1.ClientFrame{frame: {:heartbeat, %V1.Heartbeat{nonce: 7}}}
      raw = V1.ClientFrame.encode(frame)

      assert %Message{topic: "session", event: "frame", payload: ^frame} =
               Serializer.decode!(raw, opcode: :binary)
    end

    test "a text frame selects JSON for the connection" do
      json =
        Lumia.ProtoJSON.encode!(%V1.ClientFrame{frame: {:heartbeat, %V1.Heartbeat{nonce: 1}}})

      assert %Message{event: "frame"} = Serializer.decode!(json, opcode: :text)

      assert {:socket_push, :text, _} =
               Serializer.encode!(%Message{
                 event: "frame",
                 payload: %V1.ServerFrame{frame: {:heartbeat, %V1.Heartbeat{nonce: 1}}}
               })
    end
  end

  describe "encode!/1" do
    test "the join reply is a bare Welcome frame" do
      welcome = %V1.Welcome{recovery_generation: 3}

      {:socket_push, :binary, bytes} =
        Serializer.encode!(%Reply{
          status: :ok,
          payload: welcome,
          topic: "session",
          ref: "1",
          join_ref: "1"
        })

      assert %V1.ServerFrame{frame: {:welcome, ^welcome}} =
               V1.ServerFrame.decode(IO.iodata_to_binary(bytes))
    end

    test "event bytes are wrapped without re-encoding and decode as an event frame" do
      event = %V1.Event{
        seq: 42,
        channel_id: Proto.uuid(Ecto.UUID.generate()),
        payload: {:acl_changed, %V1.AclChanged{acl_version: 2}}
      }

      bytes = V1.Event.encode(event)

      {:socket_push, :binary, iodata} =
        Serializer.encode!(%Message{event: "frame", payload: {:event_bytes, bytes}})

      wire = IO.iodata_to_binary(iodata)

      assert %V1.ServerFrame{frame: {:event, ^event}} = V1.ServerFrame.decode(wire)
      assert wire == V1.ServerFrame.encode(%V1.ServerFrame{frame: {:event, event}})
    end

    test "long event bytes get a multi-byte varint length" do
      body = String.duplicate("x", 300)

      event = %V1.Event{
        seq: 1,
        payload: {:message_created, %V1.MessageCreated{message: %V1.Message{body: body}}}
      }

      bytes = V1.Event.encode(event)

      {:socket_push, :binary, iodata} =
        Serializer.encode!(%Message{event: "frame", payload: {:event_bytes, bytes}})

      assert %V1.ServerFrame{frame: {:event, ^event}} =
               V1.ServerFrame.decode(IO.iodata_to_binary(iodata))
    end

    test "phx_close and phx_error become Disconnect frames" do
      {:socket_push, :binary, bytes} =
        Serializer.encode!(%Message{event: "phx_close", topic: "session"})

      assert %V1.ServerFrame{
               frame: {:disconnect, %V1.Disconnect{reason: :DISCONNECT_SERVER_SHUTDOWN}}
             } = V1.ServerFrame.decode(IO.iodata_to_binary(bytes))

      {:socket_push, :binary, bytes} =
        Serializer.encode!(%Message{event: "phx_error", topic: "session"})

      assert %V1.ServerFrame{
               frame: {:disconnect, %V1.Disconnect{reason: :DISCONNECT_PROTOCOL_ERROR}}
             } = V1.ServerFrame.decode(IO.iodata_to_binary(bytes))
    end
  end
end
