defmodule LumiaWeb.AuthController do
  @moduledoc """
  Account creation with a password, password sign-in, refresh, sign-out,
  and the session list (docs/protocol.md section 9, auth table). Identity
  proof flows arrive with the identity endpoints.
  """

  use Phoenix.Controller, formats: []

  alias Lumia.Accounts
  alias Lumia.Accounts.Account
  alias Lumia.Auth
  alias Lumia.Auth.MagicLink
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.V1
  alias LumiaWeb.ProtoConn

  action_fallback LumiaWeb.FallbackController

  # An invite code is a way in when registration is closed (architecture
  # 6.2, "an email invite that creates a new identity"); the account joins
  # the invite's server in the same request.
  def create_account(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.CreateAccountRequest),
         {:ok, invite} <- admitted(req.invite_code),
         {:ok, account} <-
           Accounts.register(%{
             handle: req.handle,
             email: blank_to_nil(req.email),
             password: req.password
           }),
         {:ok, _} <- join(invite, account.id),
         {:ok, _session, tokens} <-
           Auth.create_session(account.id, device_id(req), client: client(conn)) do
      ProtoConn.render(conn, 201, token_pair(tokens))
    end
  end

  @doc false
  def admitted(""), do: with(:ok <- registration_open(), do: {:ok, nil})
  def admitted(nil), do: admitted("")

  def admitted(code) do
    case Lumia.Invites.valid(code) do
      {:ok, invite} -> {:ok, invite}
      {:error, :not_found} -> {:error, :not_found}
    end
  end

  @doc false
  def join(nil, _account_id), do: {:ok, nil}
  def join(invite, account_id), do: Lumia.Invites.accept(invite.code, account_id)

  def magic_link(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.MagicLinkRequest),
         :ok <- MagicLink.request(req.email) do
      send_resp(conn, 202, "")
    end
  end

  def magic_link_complete(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.MagicLinkCompleteRequest),
         {:ok, _session, tokens} <-
           MagicLink.complete(
             req.token,
             Proto.from_uuid(req.device_id) || Ecto.UUID.generate(),
             client: client(conn)
           ) do
      ProtoConn.render(conn, token_pair(tokens))
    end
  end

  def sign_in_password(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.PasswordSignInRequest),
         {:ok, account} <- Accounts.authenticate(req.handle_or_email, req.password),
         :ok <- Accounts.check_second_factor(account, req.totp_code, passkey_factor(req)),
         {:ok, _session, tokens} <-
           Auth.create_session(account.id, Proto.from_uuid(req.device_id) || Ecto.UUID.generate(),
             client: client(conn)
           ) do
      ProtoConn.render(conn, token_pair(tokens))
    end
  end

  defp passkey_factor(%V1.PasswordSignInRequest{
         passkey_challenge_id: "",
         passkey_credential_json: ""
       }),
       do: nil

  defp passkey_factor(%V1.PasswordSignInRequest{} = req),
    do: {req.passkey_challenge_id, req.passkey_credential_json}

  def totp_enroll(conn, _params) do
    with {:ok, enrolment} <- Accounts.enroll_totp(conn.assigns.current_account_id) do
      ProtoConn.render(conn, %V1.TotpEnrollResponse{
        secret: enrolment.secret,
        otpauth_uri: enrolment.otpauth_uri
      })
    end
  end

  def totp_confirm(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.TotpConfirmRequest),
         :ok <- Accounts.confirm_totp(conn.assigns.current_account_id, req.code) do
      ProtoConn.no_content(conn)
    end
  end

  def refresh(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.RefreshRequest),
         {:ok, _session, tokens} <- Auth.refresh(req.refresh_token) do
      ProtoConn.render(conn, token_pair(tokens))
    end
  end

  # POST /auth/handoff: a one-time sign-in token for the caller's account,
  # for another device of the person's (a native app's browser sheet, or
  # the app itself) to complete as a magic-link token.
  def handoff(conn, _params) do
    account = Repo.get!(Account, conn.assigns.current_account_id)

    with {:ok, token} <- MagicLink.mint(account) do
      expires_at = DateTime.add(DateTime.utc_now(), MagicLink.ttl_seconds(), :second)

      ProtoConn.render(conn, %V1.HandoffResponse{
        token: token,
        expires_at: Proto.timestamp(expires_at)
      })
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

  @doc false
  def token_pair(tokens) do
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

  # The session belongs to the device that made the account, so that revoking
  # the device ends it: the proof's device, else the one the request names. A
  # client that names none gets a session no device revocation reaches.
  defp device_id(%V1.CreateAccountRequest{proof: %V1.IdentityProof{device_id: id}})
       when byte_size(id) == 16,
       do: Proto.from_uuid(id)

  defp device_id(%V1.CreateAccountRequest{device_id: id}) when byte_size(id) == 16,
    do: Proto.from_uuid(id)

  defp device_id(_), do: Ecto.UUID.generate()

  defp client(conn), do: conn |> Plug.Conn.get_req_header("user-agent") |> List.first()
  defp blank_to_nil(""), do: nil
  defp blank_to_nil(v), do: v
end
