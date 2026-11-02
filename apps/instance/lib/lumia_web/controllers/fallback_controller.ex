defmodule LumiaWeb.FallbackController do
  @moduledoc "Maps `{:error, reason}` from contexts to the protocol's error envelope."

  use Phoenix.Controller, formats: []

  alias LumiaWeb.ProtoConn

  def call(conn, {:error, %Ecto.Changeset{} = changeset}) do
    details =
      changeset
      |> Ecto.Changeset.traverse_errors(fn {msg, _} -> msg end)
      |> Enum.map(fn {field, msgs} -> {"field", "#{field}: #{Enum.join(msgs, ", ")}"} end)
      |> Map.new()

    ProtoConn.render_error(conn, :invalid_argument, details)
  end

  def call(conn, {:error, reason}), do: ProtoConn.render_error(conn, reason)
end
