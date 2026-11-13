defmodule LumiaWeb.AccountController do
  @moduledoc "The caller's account snapshot and account log (docs/protocol.md section 7)."

  use Phoenix.Controller, formats: []

  alias Lumia.Snapshots
  alias Lumia.V1
  alias LumiaWeb.ProtoConn

  action_fallback LumiaWeb.FallbackController

  def snapshot(conn, _params) do
    ProtoConn.render(conn, Snapshots.account(conn.assigns.current_account_id))
  end

  @doc """
  `PATCH /account`: the caller's own handle and display name, either or
  both (`Lumia.Accounts.update_account/2`).
  """
  def update(conn, _params) do
    account = Lumia.Repo.get!(Lumia.Accounts.Account, conn.assigns.current_account_id)

    with {:ok, req} <- ProtoConn.decode(conn, Lumia.V1.UpdateAccountRequest),
         changes = Map.reject(Map.take(req, [:handle, :display_name]), fn {_, v} -> is_nil(v) end),
         {:ok, updated} <- Lumia.Accounts.update_account(account, changes) do
      ProtoConn.render(conn, Lumia.Proto.account(updated, own: true))
    end
  end

  @doc """
  `PUT /account/avatar`: the request's body is a picture, as chosen (PNG,
  JPEG, WebP or GIF, whatever the content type says); the account's avatar
  becomes the instance's own square of it (`Lumia.Accounts.set_avatar/2`).
  """
  def put_avatar(conn, _params) do
    account = Lumia.Repo.get!(Lumia.Accounts.Account, conn.assigns.current_account_id)

    with {:ok, upload, conn} <- LumiaWeb.Pictures.read(conn),
         {:ok, updated} <- Lumia.Accounts.set_avatar(account, upload) do
      ProtoConn.render(conn, Lumia.Proto.account(updated, own: true))
    else
      other -> LumiaWeb.Pictures.refused(conn, other)
    end
  end

  @doc "`DELETE /account/avatar`: the account goes back to having no picture."
  def delete_avatar(conn, _params) do
    account = Lumia.Repo.get!(Lumia.Accounts.Account, conn.assigns.current_account_id)

    with {:ok, updated} <- Lumia.Accounts.remove_avatar(account) do
      ProtoConn.render(conn, Lumia.Proto.account(updated, own: true))
    end
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

  @doc """
  `GET /account/mentions`: the messages that notify the caller, newest
  first, in the server `server_id` names or in all; `before` is the last
  message id of the page before (architecture 6.6).
  """
  def mentions(conn, params) do
    with {:ok, server_id} <- optional_uuid(params["server_id"]),
         {:ok, before} <- optional_uuid(params["before"]) do
      {messages, more} = Lumia.Reads.mentions(conn.assigns.current_account_id, server_id, before)
      ProtoConn.render(conn, %V1.ListMentionsResponse{messages: messages, has_more: more})
    end
  end

  defp optional_uuid(nil), do: {:ok, nil}
  defp optional_uuid(""), do: {:ok, nil}

  defp optional_uuid(value) do
    case Ecto.UUID.cast(value) do
      {:ok, id} -> {:ok, id}
      :error -> {:error, :invalid_argument}
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
