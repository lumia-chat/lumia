defmodule LumiaWeb.OidcController do
  @moduledoc "OIDC sign-in (architecture 6.3): to the provider, and back with a sign-in token."

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

  # GET /auth/oidc/callback: back from the provider, on to the client with a token.
  def callback(conn, params) do
    case Oidc.callback(params) do
      {:ok, url} -> redirect(conn, external: url)
      {:error, url} when is_binary(url) -> redirect(conn, external: url)
      {:error, nil} -> ProtoConn.render_error(conn, :unauthenticated)
    end
  end
end
