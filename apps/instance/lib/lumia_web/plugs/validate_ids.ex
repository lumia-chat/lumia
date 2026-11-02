defmodule LumiaWeb.Plugs.ValidateIds do
  @moduledoc """
  Rejects requests whose `*_id` path parameters are not UUIDs with
  `NOT_FOUND`, before any controller runs. A malformed id can never name
  a resource, and answering 404 rather than 400 keeps the response the
  same as for a well-formed id the caller may not see (protocol §10).
  """

  import Plug.Conn

  alias LumiaWeb.ProtoConn

  def init(opts), do: opts

  def call(conn, _opts) do
    bad? =
      Enum.any?(conn.path_params, fn
        {key, value} when is_binary(value) ->
          String.ends_with?(key, "_id") and Ecto.UUID.cast(value) == :error

        _ ->
          false
      end)

    if bad?, do: conn |> ProtoConn.render_error(:not_found) |> halt(), else: conn
  end
end
