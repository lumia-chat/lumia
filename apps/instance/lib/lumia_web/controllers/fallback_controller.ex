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

  # An older roster than held (identity spec 5): the held one travels back
  # in `details.held`, base64 of the `SignedRoster`, as protocol section 9 says.
  def call(conn, {:error, {:conflict, %Lumia.Identity.Roster{signed: signed}}}) do
    held = Base.encode64(Lumia.V1.SignedRoster.encode(signed))
    ProtoConn.render_error(conn, :conflict, %{"held" => held})
  end

  def call(conn, {:error, reason}), do: ProtoConn.render_error(conn, reason)
end
