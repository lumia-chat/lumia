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

  # A second factor is required: the client asks for it and calls again.
  def call(conn, {:error, {:failed_precondition, :mfa}}),
    do: ProtoConn.render_error(conn, :failed_precondition, %{"mfa" => "totp"})

  def call(conn, {:error, {:failed_precondition, :passkey}}),
    do: ProtoConn.render_error(conn, :failed_precondition, %{"mfa" => "passkey"})

  def call(conn, {:error, :resource_exhausted}),
    do:
      ProtoConn.render_error(conn, :resource_exhausted, %{
        "quota" => Integer.to_string(Lumia.History.quota_bytes())
      })

  def call(conn, {:error, {:resource_exhausted, state, quota}}),
    do:
      ProtoConn.render_error(conn, :resource_exhausted, %{
        "state" => state,
        "quota" => Integer.to_string(quota)
      })

  # A server at its custom emoji limit (specs/emoji.md 4).
  def call(conn, {:error, {:emoji_limit, limit}}),
    do:
      ProtoConn.render_error(conn, :resource_exhausted, %{
        "state" => "emoji",
        "limit" => Integer.to_string(limit)
      })

  # A name another holds, named when the caller may see it.
  def call(conn, {:error, {:already_exists, existing_id}}) when is_binary(existing_id),
    do: ProtoConn.render_error(conn, :already_exists, %{"existing_id" => existing_id})

  def call(conn, {:error, {:payload_too_large, max}}),
    do: ProtoConn.render_error(conn, :payload_too_large, %{"max" => Integer.to_string(max)})

  def call(conn, {:error, {:invalid_argument, field}}) when is_binary(field),
    do: ProtoConn.render_error(conn, :invalid_argument, %{"field" => field})

  def call(conn, {:error, reason}), do: ProtoConn.render_error(conn, reason)
end
