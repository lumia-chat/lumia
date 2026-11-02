defmodule LumiaWeb.Gateway.Serializer do
  @moduledoc """
  Maps bare `ClientFrame` / `ServerFrame` bytes onto the one Phoenix channel
  per socket (docs/specs/delivery.md section 5). No Phoenix envelope reaches
  the wire.

    * A binary WebSocket frame is a serialized `Lumia.V1.ClientFrame`; a
      text frame is its ProtoJSON form. The encoding of the first frame the
      socket receives is the encoding for the connection.
    * `Hello` is presented to Phoenix as the join of topic `session`.
      Every other client frame is the `frame` event on that topic.
    * The join reply carries `Welcome` (or an `Error`). Every other push
      carries either a `Lumia.V1.ServerFrame` struct or `{:event_bytes, bin}`,
      the stored bytes of a `Lumia.V1.Event`, which a protobuf session wraps
      without decoding (encode once, delivery section 4).

  Decode and encode both run in the transport process, so the connection's
  encoding is kept in that process's dictionary.
  """

  @behaviour Phoenix.Socket.Serializer

  import Bitwise, only: [>>>: 2]

  alias Lumia.V1
  alias Phoenix.Socket.Broadcast
  alias Phoenix.Socket.Message
  alias Phoenix.Socket.Reply

  @topic "session"
  @join_ref "1"
  # ServerFrame.event is field 3, wire type 2 (length-delimited).
  @event_field_tag <<0x1A>>

  @impl Phoenix.Socket.Serializer
  def decode!(raw, opts) do
    encoding = encoding_for(Keyword.get(opts, :opcode, :binary))

    frame =
      case encoding do
        :proto -> V1.ClientFrame.decode(raw)
        :json -> Protobuf.JSON.decode!(raw, V1.ClientFrame)
      end

    case frame do
      %V1.ClientFrame{frame: {:hello, hello}} ->
        Process.put(:lumia_gateway_hello_seen, true)

        %Message{
          topic: @topic,
          event: "phx_join",
          payload: hello,
          ref: @join_ref,
          join_ref: @join_ref
        }

      %V1.ClientFrame{} ->
        # Anything before Hello is a protocol error (protocol section 6).
        # Phoenix answers the unjoined topic with an error reply, which
        # `encode!/1` turns into the Disconnect; the "disconnect" broadcast
        # queued here then stops the transport, exactly as an endpoint
        # `broadcast(socket_id, "disconnect", %{})` would.
        unless Process.get(:lumia_gateway_hello_seen) do
          send(self(), %Broadcast{topic: @topic, event: "disconnect", payload: %{}})
        end

        %Message{topic: @topic, event: "frame", payload: frame, ref: nil, join_ref: @join_ref}
    end
  end

  @impl Phoenix.Socket.Serializer
  def encode!(%Reply{payload: payload}) do
    frame =
      case payload do
        %V1.Welcome{} = welcome -> %V1.ServerFrame{frame: {:welcome, welcome}}
        %V1.Error{} = error -> %V1.ServerFrame{frame: {:error, error}}
        %V1.ServerFrame{} = frame -> frame
        # Phoenix's own replies (an unmatched topic after the channel went
        # away) have no place in the protocol: they become a disconnect.
        %{reason: _} -> protocol_error_frame()
      end

    push(frame)
  end

  def encode!(%Message{event: "frame", payload: {:event_bytes, bytes}}) do
    case encoding() do
      :proto -> {:socket_push, :binary, [@event_field_tag, varint(byte_size(bytes)), bytes]}
      :json -> push(%V1.ServerFrame{frame: {:event, V1.Event.decode(bytes)}})
    end
  end

  def encode!(%Message{event: "frame", payload: %V1.ServerFrame{} = frame}), do: push(frame)

  # Phoenix closes the channel: tell the client why in our own vocabulary.
  def encode!(%Message{event: "phx_close"}) do
    push(%V1.ServerFrame{
      frame:
        {:disconnect,
         %V1.Disconnect{reason: :DISCONNECT_SERVER_SHUTDOWN, reconnect_after_seconds: 1}}
    })
  end

  def encode!(%Message{event: "phx_error"}), do: protocol_error()

  @impl Phoenix.Socket.Serializer
  def fastlane!(%Broadcast{}), do: raise("the gateway does not use Phoenix broadcasts")

  defp protocol_error, do: push(protocol_error_frame())

  defp protocol_error_frame do
    %V1.ServerFrame{
      frame: {:disconnect, %V1.Disconnect{reason: :DISCONNECT_PROTOCOL_ERROR}}
    }
  end

  defp push(%V1.ServerFrame{} = frame) do
    case encoding() do
      :proto -> {:socket_push, :binary, V1.ServerFrame.encode(frame)}
      :json -> {:socket_push, :text, Protobuf.JSON.encode!(frame)}
    end
  end

  # The first decoded frame fixes the connection's encoding.
  defp encoding_for(opcode) do
    case Process.get(:lumia_gateway_encoding) do
      nil ->
        encoding = if opcode == :text, do: :json, else: :proto
        Process.put(:lumia_gateway_encoding, encoding)
        encoding

      encoding ->
        encoding
    end
  end

  defp encoding, do: Process.get(:lumia_gateway_encoding, :proto)

  defp varint(n) when n < 128, do: <<n>>
  defp varint(n), do: <<1::1, n::7, varint(n >>> 7)::binary>>
end
