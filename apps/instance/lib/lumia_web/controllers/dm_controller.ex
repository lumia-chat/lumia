defmodule LumiaWeb.DmController do
  @moduledoc "Personal DMs (docs/protocol.md section 9): `POST /dms/personal`."

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
end
