defmodule LumiaWeb.UploadController do
  @moduledoc """
  Attachment blobs (docs/protocol.md "Uploads", specs/media.md 4): a slot
  for one (`POST /uploads`), the client's word that its ciphertext is in
  the store (`POST /uploads/{blob_id}/complete`), and, for a directory
  store, the upload itself (`PUT /blobs/upload/{token}`, authorized by
  the token alone).
  """

  use Phoenix.Controller, formats: []

  alias Lumia.Proto
  alias Lumia.Uploads
  alias Lumia.V1
  alias LumiaWeb.ProtoConn

  action_fallback LumiaWeb.FallbackController

  def create(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.CreateUploadRequest),
         channel_id when is_binary(channel_id) <-
           Proto.from_uuid(req.channel_id) || {:error, :not_found},
         {:ok, slot} <-
           Uploads.create(conn.assigns.current_account_id, channel_id, req.role, req.size_bytes) do
      ProtoConn.render(conn, 201, %V1.CreateUploadResponse{
        blob_id: Proto.uuid(slot.upload.id),
        upload_url: slot.url,
        upload_headers: slot.headers,
        upload_expires_at: Proto.timestamp(slot.expires_at)
      })
    end
  end

  def complete(conn, %{"blob_id" => blob_id}) do
    with :ok <- Uploads.complete(conn.assigns.current_account_id, blob_id),
         do: send_resp(conn, 204, "")
  end

  def put_local(conn, %{"token" => token}) do
    with {:ok, upload} <- Uploads.for_token(token),
         {:ok, bytes, conn} <- read(conn, upload.size),
         :ok <- Uploads.put_local(upload, bytes) do
      send_resp(conn, 204, "")
    end
  end

  # One byte more than the slot, so a longer body is seen to be one.
  defp read(conn, size) do
    case Plug.Conn.read_body(conn, length: size + 1, read_length: 1_000_000) do
      {:ok, body, conn} -> {:ok, body, conn}
      {:more, _partial, _conn} -> {:error, {:invalid_argument, "size_bytes"}}
      {:error, _} -> {:error, :invalid_argument}
    end
  end
end
