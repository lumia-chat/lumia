defmodule LumiaWeb.AvatarController do
  @moduledoc """
  `GET /avatars/{address}`: an account's picture, by the address its
  `avatar_url` names. Public: the address is 256 bits nobody can compute
  without the instance master key, it is what a signed profile carries to
  other instances (which have no account here to ask with), and what it
  leads to is a picture its owner chose to be seen by. The bytes were
  encoded by this instance (`Lumia.Media`) and never change under an
  address, so they are cacheable for good.
  """

  use Phoenix.Controller, formats: []

  alias Lumia.Blobs
  alias LumiaWeb.ProtoConn

  def show(conn, %{"address" => name}) do
    with address when is_binary(address) <- Blobs.address(name),
         {:ok, %{media_type: media_type, bytes: bytes}} <- Blobs.fetch(address, "avatar") do
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
