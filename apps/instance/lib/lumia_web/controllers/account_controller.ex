defmodule LumiaWeb.AccountController do
  @moduledoc "The caller's account snapshot and account log (docs/protocol.md section 7)."

  use Phoenix.Controller, formats: []

  alias Lumia.Snapshots
  alias LumiaWeb.ProtoConn

  action_fallback LumiaWeb.FallbackController

  def snapshot(conn, _params) do
    ProtoConn.render(conn, Snapshots.account(conn.assigns.current_account_id))
  end

  def events(conn, params) do
    after_seq = int(params["after_seq"], 0)
    limit = int(params["limit"], 200)

    ProtoConn.render(
      conn,
      Snapshots.account_events(conn.assigns.current_account_id, after_seq, limit)
    )
  end

  defp int(nil, default), do: default

  defp int(value, default) do
    case Integer.parse(value) do
      {n, ""} when n >= 0 -> n
      _ -> default
    end
  end
end
