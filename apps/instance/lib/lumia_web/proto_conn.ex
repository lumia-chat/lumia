defmodule LumiaWeb.ProtoConn do
  @moduledoc """
  Request decoding and response rendering for the API's two encodings
  (docs/protocol.md section 2), and the error envelope (section 10).

  A request body is decoded into the `Lumia.V1` struct the endpoint
  expects, from JSON (ProtoJSON) or protobuf according to its content
  type. A response is rendered as protobuf when the client sends
  `Accept: application/x-protobuf`, else as JSON.
  """

  import Plug.Conn

  alias Lumia.V1

  @proto "application/x-protobuf"
  @json "application/json"

  @doc "Decodes the request body as `module`. An empty body decodes to the default struct."
  def decode(conn, module) do
    case conn.private[:lumia_raw_body] do
      nil -> {:ok, struct(module)}
      {_, ""} -> {:ok, struct(module)}
      {"x-protobuf", body} -> safe(fn -> module.decode(body) end)
      {"json", body} -> safe(fn -> Protobuf.JSON.decode!(body, module) end)
    end
  end

  defp safe(fun) do
    {:ok, fun.()}
  rescue
    _ -> {:error, :invalid_argument}
  end

  @doc "Renders a `Lumia.V1` struct with the given status."
  def render(conn, status \\ 200, struct) do
    if wants_proto?(conn) do
      conn
      |> put_resp_content_type(@proto)
      |> send_resp(status, struct.__struct__.encode(struct))
    else
      conn
      |> put_resp_content_type(@json)
      |> send_resp(status, Protobuf.JSON.encode!(struct))
    end
  end

  @doc "Renders an empty 204."
  def no_content(conn), do: send_resp(conn, 204, "")

  @doc "Renders the error envelope for an error reason."
  def render_error(conn, reason, details \\ %{}) do
    {code, status} = code_and_status(reason)
    error = %V1.Error{code: code, message: message(reason), details: details}
    render(conn, status, error)
  end

  # The protocol's error table (section 10).
  defp code_and_status(:invalid_argument), do: {:INVALID_ARGUMENT, 400}
  defp code_and_status(:failed_precondition), do: {:FAILED_PRECONDITION, 400}
  defp code_and_status(:unauthenticated), do: {:UNAUTHENTICATED, 401}
  defp code_and_status(:token_expired), do: {:TOKEN_EXPIRED, 401}
  defp code_and_status(:session_revoked), do: {:SESSION_REVOKED, 401}
  defp code_and_status(:device_revoked), do: {:DEVICE_REVOKED, 401}
  defp code_and_status(:permission_denied), do: {:PERMISSION_DENIED, 403}
  defp code_and_status(:policy_denied), do: {:POLICY_DENIED, 403}
  defp code_and_status(:not_found), do: {:NOT_FOUND, 404}
  defp code_and_status(:channel_not_found), do: {:NOT_FOUND, 404}
  defp code_and_status(:message_not_found), do: {:NOT_FOUND, 404}
  defp code_and_status(:account_not_found), do: {:NOT_FOUND, 404}
  defp code_and_status(:conflict), do: {:CONFLICT, 409}
  defp code_and_status(:already_exists), do: {:ALREADY_EXISTS, 409}
  defp code_and_status(:limit_exceeded), do: {:FAILED_PRECONDITION, 400}
  defp code_and_status(:rate_limited), do: {:RATE_LIMITED, 429}
  defp code_and_status(_), do: {:INTERNAL, 500}

  defp message(reason) when is_atom(reason),
    do: reason |> Atom.to_string() |> String.replace("_", " ")

  defp message(_), do: "internal"

  defp wants_proto?(conn) do
    conn |> get_req_header("accept") |> Enum.any?(&String.contains?(&1, @proto))
  end
end
