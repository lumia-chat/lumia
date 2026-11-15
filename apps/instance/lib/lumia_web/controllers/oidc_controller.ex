defmodule LumiaWeb.OidcController do
  @moduledoc """
  OIDC sign-in (architecture 6.3): to the provider, and back with a sign-in
  token; and reauth by a fresh login there (specs/identity.md 10).
  """

  use Phoenix.Controller, formats: []

  alias Lumia.Auth.Oidc
  alias LumiaWeb.ProtoConn

  action_fallback LumiaWeb.FallbackController

  # GET /auth/oidc/start?return={url}: off to the provider.
  def start(conn, params) do
    case Oidc.start(params["return"] || "") do
      {:ok, url} ->
        redirect(conn, external: url)

      {:error, {:provider, message}} ->
        ProtoConn.render_error(conn, :internal, %{"provider" => message})

      {:error, reason} ->
        ProtoConn.render_error(conn, reason)
    end
  end

  # POST /auth/reauth/oidc/start: the provider's URL for a fresh login, its
  # callback landing with a reauth token for this session.
  def reauth_start(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, Lumia.V1.ReauthOidcStartRequest) do
      case Oidc.start_reauth(conn.assigns.current_session, req.return_to) do
        {:ok, url} ->
          ProtoConn.render(conn, %Lumia.V1.ReauthOidcStartResponse{url: url})

        {:error, {:provider, message}} ->
          ProtoConn.render_error(conn, :internal, %{"provider" => message})

        {:error, reason} ->
          {:error, reason}
      end
    end
  end

  # GET /auth/oidc/callback: back from the provider, on to the client with a token.
  def callback(conn, params) do
    case Oidc.callback(params) do
      {:ok, url} -> redirect(conn, external: url)
      {:error, url} when is_binary(url) -> redirect(conn, external: url)
      {:error, nil} -> ProtoConn.render_error(conn, :unauthenticated)
    end
  end
end
