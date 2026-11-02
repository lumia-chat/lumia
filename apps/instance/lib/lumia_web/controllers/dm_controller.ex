defmodule LumiaWeb.DmController do
  @moduledoc """
  Personal DMs and DM requests (docs/protocol.md section 9): `POST
  /dms/personal`, `POST /dm-requests`, `POST /dm-requests/{id}/respond`.
  """

  use Phoenix.Controller, formats: []

  alias Lumia.Dms
  alias Lumia.Proto
  alias Lumia.V1
  alias LumiaWeb.ProtoConn

  action_fallback LumiaWeb.FallbackController

  def open_personal(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.OpenPersonalDmRequest),
         ids = Enum.map(req.participant_account_ids, &Proto.from_uuid/1),
         {:ok, channel, existed} <- Dms.open_personal(conn.assigns.current_account_id, ids) do
      response = %V1.OpenPersonalDmResponse{channel: Proto.channel(channel), existed: existed}
      ProtoConn.render(conn, if(existed, do: 200, else: 201), response)
    end
  end

  def send_request(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.SendDmRequestRequest),
         to when is_binary(to) <-
           Proto.from_uuid(req.to_account_id) || {:error, :invalid_argument},
         {:ok, request} <- Dms.send_request(conn.assigns.current_account_id, to) do
      ProtoConn.render(conn, 201, Proto.dm_request(request))
    end
  end

  def respond(conn, %{"request_id" => request_id}) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.RespondDmRequestRequest),
         {:ok, state} <- response_state(req.response),
         {:ok, request, _channel} <-
           Dms.respond(request_id, conn.assigns.current_account_id, state) do
      ProtoConn.render(conn, Proto.dm_request(request))
    end
  end

  defp response_state(:ACCEPTED), do: {:ok, :accepted}
  defp response_state(:DECLINED), do: {:ok, :declined}
  defp response_state(:BLOCKED), do: {:ok, :blocked}
  defp response_state(_), do: {:error, :invalid_argument}
end
