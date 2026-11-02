defmodule LumiaWeb.ChannelController do
  @moduledoc """
  Channel reads and the command endpoint (docs/protocol.md section 9).
  Every action resolves access authoritatively first; a non-member gets
  `NOT_FOUND`.
  """

  use Phoenix.Controller, formats: []

  alias Lumia.Authz.Access
  alias Lumia.Events
  alias Lumia.Proto
  alias Lumia.Servers
  alias Lumia.Snapshots
  alias Lumia.V1
  alias LumiaWeb.ProtoConn

  action_fallback LumiaWeb.FallbackController

  @write_permission %{
    send_message: :send_messages,
    edit_message: :send_messages,
    delete_message: :send_messages,
    add_reaction: :add_reactions,
    remove_reaction: :add_reactions,
    pin_message: :manage_messages,
    unpin_message: :manage_messages,
    send_encrypted: :send_messages
  }

  def create(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.CreateChannelRequest),
         server_id = Proto.from_uuid(req.server_id),
         {:ok, channel} <-
           Servers.create_channel(server_id, conn.assigns.current_account_id, req.name,
             topic: req.topic
           ) do
      ProtoConn.render(conn, 201, Proto.channel(channel))
    end
  end

  def events(conn, %{"channel_id" => channel_id} = params) do
    with {:ok, access} <-
           Access.require(conn.assigns.current_account_id, channel_id, :read_history) do
      after_seq = int(params["after_seq"], 0)
      limit = min(int(params["limit"], 200), 200)
      events = Events.events_after(channel_id, after_seq, limit)

      ProtoConn.render(conn, %V1.ListEventsResponse{
        events: events,
        last_seq: access.channel.last_seq,
        truncated_before: false
      })
    end
  end

  def snapshot(conn, %{"channel_id" => channel_id}) do
    with {:ok, access} <-
           Access.require(conn.assigns.current_account_id, channel_id, :read_history) do
      ProtoConn.render(conn, Snapshots.channel(access))
    end
  end

  def messages(conn, %{"channel_id" => channel_id} = params) do
    with {:ok, _} <- Access.require(conn.assigns.current_account_id, channel_id, :read_history) do
      parent_id = if params["parent_id"] not in [nil, ""], do: params["parent_id"]

      response =
        Snapshots.messages(
          channel_id,
          int(params["before_seq"], 0),
          int(params["limit"], 50),
          parent_id
        )

      ProtoConn.render(conn, response)
    end
  end

  def command(conn, %{"channel_id" => channel_id}) do
    actor = %{
      account_id: conn.assigns.current_account_id,
      device_id: conn.assigns.current_device_id
    }

    with {:ok, %V1.Command{op: {op, _}} = command} <- decode_command(conn, channel_id),
         {:ok, _} <-
           Access.require(actor.account_id, channel_id, Map.fetch!(@write_permission, op)),
         {:ok, ack} <- Events.append(channel_id, actor, command) do
      ProtoConn.render(conn, %V1.Ack{
        command_id: command.command_id,
        channel_id: command.channel_id,
        seq: ack.seq,
        event_id: Proto.uuid(ack.event_id)
      })
    end
  end

  # The path's channel wins over the body's, and the command must have an op.
  defp decode_command(conn, channel_id) do
    case ProtoConn.decode(conn, V1.Command) do
      {:ok, %V1.Command{op: {_, _}} = command} ->
        {:ok, %{command | channel_id: Proto.uuid(channel_id)}}

      {:ok, _} ->
        {:error, :invalid_argument}

      error ->
        error
    end
  end

  defp int(nil, default), do: default

  defp int(value, default) do
    case Integer.parse(value) do
      {n, ""} when n >= 0 -> n
      _ -> default
    end
  end
end
