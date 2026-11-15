defmodule LumiaWeb.BlobController do
  @moduledoc """
  An attachment blob's ciphertext (`GET /blobs/{blob_id}`), for a caller
  who can read the message naming it: a redirect to a presigned URL on the
  bucket, or the bytes from a directory store. And its removal by its
  uploader (`DELETE /blobs/{blob_id}`). specs/media.md 4.3 and 4.4.
  """

  use Phoenix.Controller, formats: []

  alias Lumia.Uploads

  action_fallback LumiaWeb.FallbackController

  def show(conn, %{"blob_id" => blob_id}) do
    case Uploads.fetch(conn.assigns.current_account_id, blob_id) do
      # Never kept: a redirect cached past a delete would still lead to the
      # bytes (specs/media.md 4.3).
      {:redirect, url} ->
        conn
        |> put_resp_header("location", url)
        |> put_resp_header("cache-control", "no-store")
        |> send_resp(302, "")

      {:bytes, bytes} ->
        conn
        |> put_resp_header("content-type", "application/octet-stream")
        |> put_resp_header("cache-control", "private, max-age=300")
        |> put_resp_header("x-content-type-options", "nosniff")
        |> send_resp(200, bytes)

      error ->
        error
    end
  end

  def delete(conn, %{"blob_id" => blob_id}) do
    with :ok <- Uploads.delete(conn.assigns.current_account_id, blob_id),
         do: send_resp(conn, 204, "")
  end
end
