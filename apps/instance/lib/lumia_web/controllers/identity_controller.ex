defmodule LumiaWeb.IdentityController do
  @moduledoc "Identity endpoints (docs/protocol.md section 9, auth and identity table)."

  use Phoenix.Controller, formats: []

  alias Lumia.Accounts
  alias Lumia.Auth
  alias Lumia.Identity
  alias Lumia.Identity.LinkSessions
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

  # The held roster whose current root is `root` (base64url), for a device
  # recovering from a phrase after a rotation: the phrase derives the
  # current root, not the first one the identity id is the hash of
  # (identity spec 4.3).
  def get_roster_by_root(conn, %{"root" => root}) do
    with {:ok, root} <- Base.url_decode64(root, padding: false) |> or_invalid(),
         %{signed: signed} <- Identity.held_roster_by_root(root) || {:error, :not_found} do
      ProtoConn.render(conn, %V1.GetRosterResponse{roster: signed})
    end
  end

  def sign_in(conn, _params) do
    with :ok <- policy(:identity_proof_sign_in),
         {:ok, %V1.IdentityProofSignInRequest{proof: %V1.IdentityProof{} = proof}} <-
           ProtoConn.decode(conn, V1.IdentityProofSignInRequest),
         {:ok, verified, device} <- Identity.verify_proof(proof, :SIGN_IN),
         %Lumia.Accounts.Account{} = account <-
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
         {:ok, invite} <- invite(req.invite_code),
         {:ok, verified, device} <- Identity.verify_proof(proof, :CREATE_ACCOUNT),
         :ok <- no_account_yet(verified.identity_id),
         {:ok, imported} <- imported_profile(req.profile, verified),
         {:ok, account} <-
           Accounts.create_account(
             Map.merge(imported, %{handle: req.handle, identity_id: verified.identity_id})
           ),
         {:ok, _} <- LumiaWeb.AuthController.join(invite, account.id),
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
         {:ok, _account} <- credential(req, session) do
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

  @doc """
  The newest roster held for another account's identity, for validating
  that account's MLS leaves (e2ee spec section 2). Gated like a key-package
  claim: the caller must share a personal channel with the account.
  """
  def get_account_roster(conn, %{"account_id" => account_id}) do
    caller = conn.assigns.current_account_id

    with true <- Lumia.Dms.shares_personal_channel?(caller, account_id) || {:error, :not_found},
         %{identity_id: identity_id} when is_binary(identity_id) <-
           Lumia.Repo.get(Lumia.Accounts.Account, account_id) || {:error, :not_found},
         %{signed: signed} <- Identity.held_roster(identity_id) || {:error, :not_found} do
      ProtoConn.render(conn, %V1.GetRosterResponse{roster: signed})
    else
      {:error, _} = error -> error
      _ -> {:error, :not_found}
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

  # Identity spec 4.5 step 5: the rotating device hands the new backup key
  # to the identity's other devices; verified here against the held roster
  # and put on every account of the identity's log, where each device opens
  # its own copy.
  def publish_backup_key_handoff(conn, _params) do
    with {:ok, identity_id} <- bound_identity(conn),
         {:ok, %V1.PublishBackupKeyHandoffRequest{handoff: %V1.BackupKeyHandoff{} = handoff}} <-
           ProtoConn.decode(conn, V1.PublishBackupKeyHandoffRequest),
         :ok <- Identity.accept_backup_key_handoff(handoff, identity_id) do
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

  def revoke_device(conn, _params) do
    with {:ok, _identity_id} <- bound_identity(conn),
         {:ok, %V1.RevokeDeviceRequest{roster: %V1.SignedRoster{} = roster} = req} <-
           ProtoConn.decode(conn, V1.RevokeDeviceRequest),
         {:ok, _verified} <-
           Identity.accept_roster(
             roster,
             {:revoke, req.reauth_token, conn.assigns.current_session}
           ) do
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

    with :ok <- reauthed(conn, params["reauth_token"] || header(conn, "x-reauth-token")),
         {:ok, %V1.IdentityProof{} = proof} <- ProtoConn.decode(conn, V1.IdentityProof),
         {:ok, verified, _device} <- Identity.verify_proof(proof, :LINK_ACCOUNT),
         {:ok, _account} <- Identity.link_account(account_id, verified.identity_id) do
      ProtoConn.no_content(conn)
    end
  end

  # Starting over (identity spec 4.7): the account moves to the identity
  # the proof shows, under this session's reauth.
  def rebind(conn, _params) do
    with {:ok, %V1.RebindIdentityRequest{proof: %V1.IdentityProof{} = proof} = req} <-
           ProtoConn.decode(conn, V1.RebindIdentityRequest),
         :ok <- reauthed(conn, req.reauth_token),
         {:ok, verified, _device} <- Identity.verify_proof(proof, :LINK_ACCOUNT),
         {:ok, _account} <- Identity.rebind_account(conn.assigns.current_account_id, verified) do
      ProtoConn.no_content(conn)
    else
      {:ok, %V1.RebindIdentityRequest{}} -> {:error, :invalid_argument}
      other -> other
    end
  end

  # A reauth token is this session's (identity spec 10): one another session
  # earned, let alone another account's, authorizes nothing here.
  defp reauthed(conn, token) do
    case Identity.verify_reauth(token) do
      {:ok, %{id: id}} when id == conn.assigns.current_session.id -> :ok
      _ -> {:error, :unauthenticated}
    end
  end

  # ---- Linking by prompt (identity spec 4.2.1) ----------------------------------

  def open_link_session(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.OpenLinkSessionRequest),
         {:ok, link} <-
           LinkSessions.open(conn.assigns.current_session, req.request, req.commitment) do
      ProtoConn.render(conn, 201, LinkSessions.to_proto(link))
    end
  end

  def list_link_sessions(conn, _params) do
    sessions =
      conn.assigns.current_account_id
      |> LinkSessions.list_open()
      |> Enum.map(&LinkSessions.to_proto/1)

    ProtoConn.render(conn, %V1.ListLinkSessionsResponse{sessions: sessions})
  end

  def get_link_session(conn, %{"id" => id}) do
    with {:ok, link} <- LinkSessions.get(conn.assigns.current_account_id, id) do
      ProtoConn.render(conn, LinkSessions.to_proto(link))
    end
  end

  def answer_link_session(conn, %{"id" => id}) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.AnswerLinkSessionRequest),
         {:ok, link} <- LinkSessions.answer(conn.assigns.current_session, id, req.answer) do
      ProtoConn.render(conn, LinkSessions.to_proto(link))
    end
  end

  def reveal_link_session(conn, %{"id" => id}) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.RevealLinkSessionRequest),
         {:ok, link} <- LinkSessions.reveal(conn.assigns.current_session, id, req.reveal) do
      ProtoConn.render(conn, LinkSessions.to_proto(link))
    end
  end

  def link_session_code(conn, %{"id" => id}) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.LinkSessionCodeRequest),
         {:ok, link} <- LinkSessions.code(conn.assigns.current_session, id, req.code) do
      ProtoConn.render(conn, LinkSessions.to_proto(link))
    end
  end

  def decline_link_session(conn, %{"id" => id}) do
    with {:ok, link} <- LinkSessions.decline(conn.assigns.current_session, id) do
      ProtoConn.render(conn, LinkSessions.to_proto(link))
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

  defp credential(%V1.ReauthRequest{credential: {:password, password}}, session) do
    case Lumia.Repo.get(Lumia.Accounts.Account, session.account_id) do
      %{handle: handle} -> Accounts.authenticate(handle, password)
      _ -> {:error, :unauthenticated}
    end
  end

  defp credential(%V1.ReauthRequest{credential: {:totp_code, code}}, session) do
    case Lumia.Repo.get(Lumia.Accounts.Account, session.account_id) do
      %{totp_enabled_at: %DateTime{}, totp_secret: secret} = account ->
        if Lumia.Auth.Totp.valid?(secret, code),
          do: {:ok, account},
          else: {:error, :unauthenticated}

      _ ->
        {:error, :unauthenticated}
    end
  end

  # By one of this account's passkeys: an assertion by another account's
  # is as wrong as a bad one.
  defp credential(%V1.ReauthRequest{credential: {:passkey_assertion, json}} = req, session) do
    account_id = session.account_id

    case Lumia.Auth.Passkeys.verify_assertion(req.passkey_challenge_id, json) do
      {:ok, ^account_id} -> {:ok, Lumia.Repo.get!(Lumia.Accounts.Account, account_id)}
      _ -> {:error, :unauthenticated}
    end
  end

  defp credential(%V1.ReauthRequest{credential: {:magic_code, code}}, session) do
    with :ok <- Lumia.Auth.ReauthCodes.verify(session, code) do
      {:ok, Lumia.Repo.get!(Lumia.Accounts.Account, session.account_id)}
    end
  end

  defp credential(_req, _session), do: {:error, :invalid_argument}

  defp no_account_yet(identity_id) do
    if Identity.account_for(identity_id), do: {:error, :already_exists}, else: :ok
  end

  # The profile the identity carries (architecture 6.2, "profile imported"):
  # its display name and avatar become the account's, once the roster
  # vouches for the signature. None is fine; a bad one is refused.
  defp imported_profile(nil, _verified), do: {:ok, %{}}

  defp imported_profile(%V1.SignedProfile{} = signed, verified) do
    with {:ok, body} <- Identity.verify_profile(signed, verified) do
      {:ok,
       %{
         display_name: blank_to_nil(body.display_name),
         avatar_url: blank_to_nil(body.avatar_url),
         pronouns: blank_to_nil(body.pronouns),
         bio: blank_to_nil(body.bio),
         profile: V1.SignedProfile.encode(signed)
       }}
    end
  end

  defp blank_to_nil(""), do: nil
  defp blank_to_nil(v), do: v

  # The invite that led here, when the request names one: the account
  # joins its server. Identity-proof creation has its own policy, so a
  # code here only joins; an unknown one is refused before anything is made.
  defp invite(""), do: {:ok, nil}
  defp invite(nil), do: {:ok, nil}
  defp invite(code), do: Lumia.Invites.valid(code)

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
