defmodule Lumia.Relay.Router do
  @moduledoc """
  `POST /push` with `{"platform", "token", "instance", "notice"}` from an
  instance (`Lumia.Push.Sender.Http` there); `GET /health`. The body is
  bounded, the notice is reduced to its title and body, and the answer
  says only whether the token is gone, so an instance can forget it.
  """

  use Plug.Router

  plug Plug.Logger, log: :info
  plug :match
  plug Plug.Parsers, parsers: [:json], json_decoder: Jason, length: 4_096
  plug :dispatch

  get "/health" do
    send_resp(conn, 200, "ok")
  end

  post "/push" do
    with %{"platform" => platform, "token" => token, "instance" => instance} = params <-
           conn.body_params,
         notice = Map.take(Map.get(params, "notice", %{}), ["title", "body"]),
         :ok <- Lumia.Relay.deliver(platform, token, instance, notice) do
      send_resp(conn, 204, "")
    else
      {:error, :gone} -> json(conn, 410, %{"error" => "gone"})
      {:error, :invalid_argument} -> json(conn, 400, %{"error" => "invalid_argument"})
      {:error, _} -> json(conn, 502, %{"error" => "provider"})
      _ -> json(conn, 400, %{"error" => "invalid_argument"})
    end
  end

  match _ do
    send_resp(conn, 404, "")
  end

  defp json(conn, status, body) do
    conn
    |> put_resp_content_type("application/json")
    |> send_resp(status, Jason.encode!(body))
  end
end
