defmodule LumiaWeb.AccountController do
  @moduledoc "The caller's account snapshot and account log (docs/protocol.md section 7)."

  use Phoenix.Controller, formats: []

  alias Lumia.Snapshots
  alias LumiaWeb.ProtoConn

  action_fallback LumiaWeb.FallbackController

  def snapshot(conn, _params) do
    ProtoConn.render(conn, Snapshots.account(conn.assigns.current_account_id))
  end

  @doc """
  `PUT /account/profile`: a new signed profile for the caller's account,
  vouched for by the identity's held roster; the display name and avatar
  follow, and the account log says so (`ProfileUpdated`).
  """
  def put_profile(conn, _params) do
    account = Lumia.Repo.get!(Lumia.Accounts.Account, conn.assigns.current_account_id)

    with {:ok, signed} <- ProtoConn.decode(conn, Lumia.V1.SignedProfile),
         {:ok, updated} <- Lumia.Accounts.put_profile(account, signed) do
      ProtoConn.render(conn, Lumia.Proto.account(updated, own: true))
    end
  end

  @doc "Another account's public profile, when the caller may see it (`Lumia.Accounts.visible_to?/2`)."
  def show(conn, %{"account_id" => account_id}) do
    caller = conn.assigns.current_account_id

    with true <- Lumia.Accounts.visible_to?(caller, account_id) || {:error, :not_found},
         %Lumia.Accounts.Account{} = account <-
           Lumia.Repo.get(Lumia.Accounts.Account, account_id) || {:error, :not_found} do
      ProtoConn.render(conn, Lumia.Proto.account(account))
    end
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
