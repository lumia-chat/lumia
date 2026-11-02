defmodule LumiaWeb.AuthController do
  @moduledoc """
  Account creation with a password, password sign-in, refresh, sign-out,
  and the session list (docs/protocol.md section 9, auth table). Identity
  proof flows arrive with the identity endpoints.
  """

  use Phoenix.Controller, formats: []

  alias Lumia.Accounts
  alias Lumia.Auth
  alias Lumia.Proto
  alias Lumia.V1
  alias LumiaWeb.ProtoConn

  action_fallback LumiaWeb.FallbackController

  def create_account(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.CreateAccountRequest),
         :ok <- registration_open(),
         {:ok, account} <-
           Accounts.register(%{
             handle: req.handle,
             email: blank_to_nil(req.email),
             password: req.password
           }),
         {:ok, _session, tokens} <-
           Auth.create_session(account.id, device_id(req), client: client(conn)) do
      ProtoConn.render(conn, 201, token_pair(tokens))
    end
  end

  def sign_in_password(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.PasswordSignInRequest),
         {:ok, account} <- Accounts.authenticate(req.handle_or_email, req.password),
         {:ok, _session, tokens} <-
           Auth.create_session(account.id, Proto.from_uuid(req.device_id) || Ecto.UUID.generate(),
             client: client(conn)
           ) do
      ProtoConn.render(conn, token_pair(tokens))
    end
  end

  def refresh(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.RefreshRequest),
         {:ok, _session, tokens} <- Auth.refresh(req.refresh_token) do
      ProtoConn.render(conn, token_pair(tokens))
    end
  end

  def sign_out(conn, _params) do
    with :ok <- Auth.revoke(conn.assigns.current_session.id) do
      ProtoConn.no_content(conn)
    end
  end

  def list_sessions(conn, _params) do
    current = conn.assigns.current_session.id

    sessions =
      conn.assigns.current_account_id
      |> Auth.list_sessions()
      |> Enum.map(fn s ->
        %V1.Session{
          session_id: Proto.uuid(s.id),
          device_id: Proto.uuid(s.device_id),
          client: s.client || "",
          ip: s.ip || "",
          created_at: Proto.timestamp(s.inserted_at),
          last_seen_at: Proto.timestamp(s.last_seen_at),
          current: s.id == current
        }
      end)

    ProtoConn.render(conn, %V1.ListSessionsResponse{sessions: sessions})
  end

  def revoke_session(conn, %{"session_id" => id}) do
    owned? = Enum.any?(Auth.list_sessions(conn.assigns.current_account_id), &(&1.id == id))

    with true <- owned? || {:error, :not_found},
         :ok <- Auth.revoke(id) do
      ProtoConn.no_content(conn)
    end
  end

  defp token_pair(tokens) do
    now = DateTime.utc_now()

    %V1.TokenPair{
      access_token: tokens.access_token,
      access_expires_at: Proto.timestamp(DateTime.add(now, Auth.access_ttl_seconds(), :second)),
      refresh_token: tokens.refresh_token
    }
  end

  defp registration_open do
    if Application.get_env(:lumia, :instance, []) |> Keyword.get(:open_registration, true),
      do: :ok,
      else: {:error, :policy_denied}
  end

  defp device_id(%V1.CreateAccountRequest{proof: %V1.IdentityProof{device_id: id}})
       when byte_size(id) == 16,
       do: Proto.from_uuid(id)

  defp device_id(_), do: Ecto.UUID.generate()

  defp client(conn), do: conn |> Plug.Conn.get_req_header("user-agent") |> List.first()
  defp blank_to_nil(""), do: nil
  defp blank_to_nil(v), do: v
end
