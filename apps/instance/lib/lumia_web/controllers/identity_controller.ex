defmodule LumiaWeb.IdentityController do
  @moduledoc "Identity endpoints (docs/protocol.md section 9, auth and identity table)."

  use Phoenix.Controller, formats: []

  alias Lumia.Accounts
  alias Lumia.Auth
  alias Lumia.Identity
  alias Lumia.Proto
  alias Lumia.V1
  alias LumiaWeb.ProtoConn

  action_fallback LumiaWeb.FallbackController

  # ---- Public --------------------------------------------------------------------

  def challenge(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.ChallengeRequest),
         true <- req.purpose != :CHALLENGE_PURPOSE_UNSPECIFIED || {:error, :invalid_argument} do
      {bytes, expires_at} = Identity.issue_challenge(req.purpose)

      ProtoConn.render(conn, %V1.ChallengeResponse{
        challenge: bytes,
        expires_at: Proto.timestamp(expires_at)
      })
    end
  end

  def sign_in(conn, _params) do
    with :ok <- policy(:identity_proof_sign_in),
         {:ok, %V1.IdentityProofSignInRequest{proof: %V1.IdentityProof{} = proof}} <-
           ProtoConn.decode(conn, V1.IdentityProofSignInRequest),
         {:ok, verified, device} <- Identity.verify_proof(proof, :SIGN_IN),
         account when not is_nil(account) <-
           Identity.account_for(verified.identity_id) || {:error, :not_found},
         {:ok, _session, tokens} <-
           Auth.create_session(account.id, Proto.from_uuid(device.cert.device_id),
             client: client(conn)
           ) do
      ProtoConn.render(conn, token_pair(tokens))
    end
  end

  def create_account(conn, _params) do
    with :ok <- policy(:identity_proof_create),
         {:ok, %V1.CreateAccountRequest{proof: %V1.IdentityProof{} = proof} = req} <-
           ProtoConn.decode(conn, V1.CreateAccountRequest),
         {:ok, verified, device} <- Identity.verify_proof(proof, :CREATE_ACCOUNT),
         :ok <- no_account_yet(verified.identity_id),
         {:ok, account} <-
           Accounts.create_account(%{handle: req.handle, identity_id: verified.identity_id}),
         {:ok, _session, tokens} <-
           Auth.create_session(account.id, Proto.from_uuid(device.cert.device_id),
             client: client(conn)
           ) do
      ProtoConn.render(conn, 201, token_pair(tokens))
    end
  end

  def get_link_bundle(conn, %{"code" => code} = params) do
    with {:ok, device_id} <- Ecto.UUID.cast(params["device_id"] || "") |> or_invalid(),
         {:ok, bundle} <- Identity.claim_link_bundle(code, device_id) do
      ProtoConn.render(conn, %V1.GetLinkBundleResponse{
        ciphertext: bundle.ciphertext,
        sender_ephemeral_public_key: bundle.sender_ephemeral_public_key,
        nonce: bundle.nonce,
        host: Identity.host()
      })
    end
  end

  # ---- Authenticated ---------------------------------------------------------------

  def reauth(conn, _params) do
    session = conn.assigns.current_session

    with {:ok, req} <- ProtoConn.decode(conn, V1.ReauthRequest),
         {:ok, _account} <- credential(req, conn.assigns.current_account_id) do
      {token, expires_at} = Identity.issue_reauth(session)

      ProtoConn.render(conn, %V1.ReauthResponse{
        reauth_token: token,
        expires_at: Proto.timestamp(expires_at)
      })
    end
  end

  def get_roster(conn, _params) do
    with {:ok, identity_id} <- bound_identity(conn),
         %{signed: signed} <- Identity.held_roster(identity_id) || {:error, :not_found} do
      ProtoConn.render(conn, %V1.GetRosterResponse{roster: signed})
    end
  end

  def publish_roster(conn, _params) do
    with {:ok, identity_id} <- bound_identity(conn),
         {:ok, %V1.PublishRosterRequest{roster: %V1.SignedRoster{} = roster}} <-
           ProtoConn.decode(conn, V1.PublishRosterRequest),
         {:ok, verified} <- Identity.accept_roster(roster, :publish),
         true <- verified.identity_id == identity_id || {:error, :permission_denied} do
      ProtoConn.no_content(conn)
    else
      {:error, {:identity_proof_invalid, step, reason}} ->
        ProtoConn.render_error(conn, :identity_proof_invalid, %{
          "step" => to_string(step),
          "reason" => to_string(reason)
        })

      {:error, :policy_denied} ->
        ProtoConn.render_error(conn, :policy_denied, %{"reason" => "reauth_required"})

      other ->
        other
    end
  end

  def revoke_device(conn, _params) do
    account_id = conn.assigns.current_account_id

    with {:ok, _identity_id} <- bound_identity(conn),
         {:ok, %V1.RevokeDeviceRequest{roster: %V1.SignedRoster{} = roster} = req} <-
           ProtoConn.decode(conn, V1.RevokeDeviceRequest),
         {:ok, _verified} <-
           Identity.accept_roster(roster, {:revoke, req.reauth_token, account_id}) do
      ProtoConn.no_content(conn)
    else
      {:error, {:identity_proof_invalid, step, reason}} ->
        ProtoConn.render_error(conn, :identity_proof_invalid, %{
          "step" => to_string(step),
          "reason" => to_string(reason)
        })

      other ->
        other
    end
  end

  def link(conn, params) do
    account_id = conn.assigns.current_account_id

    with {:ok, _session} <-
           Identity.verify_reauth(params["reauth_token"] || header(conn, "x-reauth-token")),
         {:ok, %V1.IdentityProof{} = proof} <- ProtoConn.decode(conn, V1.IdentityProof),
         {:ok, verified, _device} <- Identity.verify_proof(proof, :LINK_ACCOUNT),
         {:ok, _account} <- Identity.link_account(account_id, verified.identity_id) do
      ProtoConn.no_content(conn)
    end
  end

  def create_link_bundle(conn, _params) do
    with {:ok, _identity_id} <- bound_identity(conn),
         {:ok, req} <- ProtoConn.decode(conn, V1.CreateLinkBundleRequest),
         {:ok, bundle} <- Identity.create_link_bundle(req) do
      ProtoConn.render(conn, 201, %V1.CreateLinkBundleResponse{
        code: bundle.code,
        expires_at: Proto.timestamp(bundle.expires_at)
      })
    end
  end

  def put_backup(conn, _params) do
    with {:ok, identity_id} <- bound_identity(conn),
         {:ok, req} <- ProtoConn.decode(conn, V1.PutIdentityBackupRequest),
         :ok <- Identity.put_backup(identity_id, req.version, req.ciphertext, req.nonce) do
      ProtoConn.no_content(conn)
    end
  end

  def get_backup(conn, _params) do
    with {:ok, identity_id} <- bound_identity(conn),
         backup when not is_nil(backup) <-
           Identity.get_backup(identity_id) || {:error, :not_found} do
      ProtoConn.render(conn, %V1.GetIdentityBackupResponse{
        version: backup.version,
        ciphertext: backup.ciphertext,
        nonce: backup.nonce
      })
    end
  end

  # ---- Helpers ---------------------------------------------------------------------

  defp bound_identity(conn) do
    case Lumia.Repo.get(Lumia.Accounts.Account, conn.assigns.current_account_id) do
      %{identity_id: id} when is_binary(id) -> {:ok, id}
      _ -> {:error, :failed_precondition}
    end
  end

  defp credential(%V1.ReauthRequest{credential: {:password, password}}, account_id) do
    case Lumia.Repo.get(Lumia.Accounts.Account, account_id) do
      %{handle: handle} -> Accounts.authenticate(handle, password)
      _ -> {:error, :unauthenticated}
    end
  end

  defp credential(_req, _account_id), do: {:error, :invalid_argument}

  defp no_account_yet(identity_id) do
    if Identity.account_for(identity_id), do: {:error, :already_exists}, else: :ok
  end

  defp policy(key) do
    if Application.get_env(:lumia, :instance, []) |> Keyword.get(key, true),
      do: :ok,
      else: {:error, :policy_denied}
  end

  defp token_pair(tokens) do
    %V1.TokenPair{
      access_token: tokens.access_token,
      access_expires_at:
        Proto.timestamp(DateTime.add(DateTime.utc_now(), Auth.access_ttl_seconds(), :second)),
      refresh_token: tokens.refresh_token
    }
  end

  defp client(conn), do: conn |> Plug.Conn.get_req_header("user-agent") |> List.first()
  defp header(conn, name), do: conn |> Plug.Conn.get_req_header(name) |> List.first()
  defp or_invalid({:ok, v}), do: {:ok, v}
  defp or_invalid(:error), do: {:error, :invalid_argument}
end
