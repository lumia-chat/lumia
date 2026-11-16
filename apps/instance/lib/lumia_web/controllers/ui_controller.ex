defmodule LumiaWeb.UiController do
  @moduledoc """
  Serves the web UI's `index.html` for every path the API does not claim,
  when a build of `apps/web` is installed under `priv/static/ui` (the
  router's catch-all). The app routes client-side, so a deep link to a
  conversation reloads to the same page. Without a build the instance
  says so in plain text, and an unknown `/api` path stays a JSON 404.

  The page carries a Content-Security-Policy of `frame-src` alone, naming
  the origins a link's embed loads from (specs/media.md 6.6), read at
  compile time from `proto/vectors/embeds/origins.json`, which a test in
  `lumia-core` holds equal to the core's `EMBED_ORIGINS`. No other
  directive yet: a fuller policy is its own work.
  """

  use LumiaWeb, :controller

  alias LumiaWeb.ProtoConn

  # From the repository's root; the image copies the file to the same place
  # relative to the instance (apps/instance/Dockerfile).
  @origins_path Path.expand("../../../../../proto/vectors/embeds/origins.json", __DIR__)
  @external_resource @origins_path
  @embed_origins @origins_path |> File.read!() |> Jason.decode!() |> Map.fetch!("origins")
  @csp "frame-src " <> Enum.join(@embed_origins, " ")

  @doc "The origins a link's embed loads from: the web app's `frame-src`."
  def embed_origins, do: @embed_origins

  def index(%{path_info: ["api" | _]} = conn, _params) do
    ProtoConn.render_error(conn, :not_found)
  end

  def index(conn, _params) do
    index = Path.join(ui_dir(), "index.html")

    if File.regular?(index) do
      conn
      |> put_resp_content_type("text/html")
      |> put_resp_header("cache-control", "no-cache")
      |> put_resp_header("content-security-policy", @csp)
      |> send_file(200, index)
    else
      conn
      |> put_resp_content_type("text/plain")
      |> send_resp(
        404,
        "This instance has no web UI installed. Connect with a Lumia app, or install the web build (apps/instance/README.md)."
      )
    end
  end

  @doc "Where an installed web build lives."
  def ui_dir, do: Application.app_dir(:lumia, "priv/static/ui")
end
