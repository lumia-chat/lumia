defmodule Lumia.Relay.Router do
  @moduledoc """
  `POST /push` with `{"platform", "token", "instance", "notice"}` from an
  instance (`Lumia.Push.Sender.Http` there); `GET /health`. The body is
  bounded, the notice is reduced to what the relay knows how to carry (its
  title and body, the app's badge, and `"kind": "read"` for a silent wake
  after the account read something elsewhere), and the answer says only
  whether the token is gone, so an instance can forget it.
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
         notice = notice(Map.get(params, "notice", %{})),
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

  # What a notice may carry through: words, a badge that is a count, and the
  # one kind there is besides a message.
  defp notice(notice) when is_map(notice) do
    words = Map.take(notice, ["title", "body"])

    badge =
      case notice["badge"] do
        n when is_integer(n) and n >= 0 and n < 100_000 -> %{"badge" => n}
        _ -> %{}
      end

    kind = if notice["kind"] == "read", do: %{"kind" => "read"}, else: %{}
    words |> Map.merge(badge) |> Map.merge(kind)
  end

  defp notice(_), do: %{}

  defp json(conn, status, body) do
    conn
    |> put_resp_content_type("application/json")
    |> send_resp(status, Jason.encode!(body))
  end
end
