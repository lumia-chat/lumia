defmodule LumiaWeb.UnfurlController do
  @moduledoc """
  Link cards in server content (specs/media.md 6.4): the instance's card
  for a link (`POST /unfurl`), which the composer shows before sending,
  and a card's picture (`GET /cards/{address}`), public by an address
  nobody can compute, as an avatar is.
  """

  use Phoenix.Controller, formats: []

  alias Lumia.Blobs
  alias Lumia.Cards
  alias Lumia.Proto
  alias Lumia.V1
  alias LumiaWeb.ProtoConn

  action_fallback LumiaWeb.FallbackController

  def create(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.UnfurlRequest),
         channel_id when is_binary(channel_id) <-
           Proto.from_uuid(req.channel_id) || {:error, :not_found},
         {:ok, card} <- Cards.unfurl(conn.assigns.current_account_id, channel_id, req.url) do
      ProtoConn.render(conn, %V1.UnfurlResponse{card: card})
    end
  end

  def show(conn, %{"address" => name}) do
    with address when is_binary(address) <- Blobs.address(name),
         {:ok, %{media_type: media_type, bytes: bytes}} <- Blobs.fetch(address, "card") do
      conn
      |> put_resp_header("content-type", media_type)
      |> put_resp_header("cache-control", "public, max-age=31536000, immutable")
      |> put_resp_header("x-content-type-options", "nosniff")
      |> put_resp_header("content-security-policy", "default-src 'none'; sandbox")
      |> put_resp_header("cross-origin-resource-policy", "cross-origin")
      |> send_resp(200, bytes)
    else
      _ -> ProtoConn.render_error(conn, :not_found)
    end
  end
end
