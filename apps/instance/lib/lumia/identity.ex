defmodule Lumia.Identity do
  @moduledoc """
  The instance side of docs/specs/identity.md: challenges and proofs
  (section 9), holding and accepting rosters with the revocation policy
  (sections 3.3 and 4.4), reauth tokens (section 10), link bundles
  (section 6), and identity backups (section 8).

  Verification itself is `Lumia.Identity.Roster`; this module stores,
  applies policy, and connects accepted rosters to sessions and the
  account log.
  """

  import Ecto.Query

  alias Lumia.Accounts
  alias Lumia.Accounts.Account
  alias Lumia.Auth
  alias Lumia.Auth.Session
  alias Lumia.Crypto
  alias Lumia.Identity.Backup
  alias Lumia.Identity.HeldRoster
  alias Lumia.Identity.LinkBundle
  alias Lumia.Identity.ReauthToken
  alias Lumia.Identity.Roster
  alias Lumia.Identity.UsedChallenge
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.Tx
  alias Lumia.V1

  @challenge_ttl_seconds 120
  @reauth_ttl_seconds 300
  @link_ttl_seconds 600
  @link_attempts 5
  @link_bundle_max_bytes 65_536
  @code_alphabet ~c"0123456789ABCDEFGHJKMNPQRSTVWXYZ"

  # ---- Challenges (section 9) ---------------------------------------------------

  @doc "Issues a challenge: serialized `IdentityChallengeBody` bytes. Stores nothing."
  def issue_challenge(purpose) when is_atom(purpose) do
    expires_at = DateTime.add(DateTime.utc_now(), @challenge_ttl_seconds, :second)

    body = %V1.IdentityChallengeBody{
      nonce: :crypto.strong_rand_bytes(32),
      host: host(),
      expires_at: Proto.timestamp(expires_at),
      purpose: purpose
    }

    mac = Crypto.hmac(challenge_key(), mac_input(body))
    {V1.IdentityChallengeBody.encode(%{body | mac: mac}), expires_at}
  end

  @doc """
  Verifies a proof end to end: challenge (mac, host, expiry, purpose,
  single use), roster (section 3.3, stored if newer), device membership,
  and the device signature. Returns the verified roster and the device.
  """
  def verify_proof(proof, purpose, opts \\ [])

  def verify_proof(
        %V1.IdentityProof{roster: %V1.SignedRoster{}, signature: %V1.Signature{}} = proof,
        purpose,
        opts
      ) do
    with {:ok, challenge} <- decode_challenge(proof.challenge),
         :ok <- check_challenge(challenge, purpose),
         {:ok, verified} <- accept_roster(proof.roster, Keyword.get(opts, :context, :proof)),
         true <- verified.identity_id == proof.identity_id || {:error, :identity_proof_invalid},
         {:ok, device} <- Map.fetch(verified.devices, proof.device_id) |> or_invalid(),
         :ok <- check_web_origin(device),
         true <-
           Crypto.verify(
             proof.challenge,
             proof.signature.signature,
             device.cert.public_key.ed25519
           ) ||
             {:error, :identity_proof_invalid},
         :ok <- consume_nonce(challenge) do
      {:ok, verified, device}
    end
  end

  # A proof without a roster or a signature cannot verify; it is a bad
  # request, not a crash.
  def verify_proof(%V1.IdentityProof{}, _purpose, _opts), do: {:error, :identity_proof_invalid}

  defp decode_challenge(bytes) do
    {:ok, V1.IdentityChallengeBody.decode(bytes)}
  rescue
    _ -> {:error, :identity_proof_invalid}
  end

  # A challenge this instance issued always carries an expiry; one without
  # is forged or truncated, and must not reach the MAC input.
  defp check_challenge(%V1.IdentityChallengeBody{expires_at: nil}, _purpose),
    do: {:error, :identity_proof_invalid}

  defp check_challenge(challenge, purpose) do
    now = DateTime.utc_now()

    cond do
      not Crypto.secure_compare(challenge.mac, Crypto.hmac(challenge_key(), mac_input(challenge))) ->
        {:error, :identity_proof_invalid}

      challenge.host != host() ->
        {:error, :identity_proof_invalid}

      challenge.purpose != purpose ->
        {:error, :identity_proof_invalid}

      DateTime.compare(Proto.from_timestamp(challenge.expires_at), now) != :gt ->
        {:error, :identity_proof_invalid}

      true ->
        :ok
    end
  end

  # Single use across every node: the primary key insert fails on replay.
  defp consume_nonce(challenge) do
    %UsedChallenge{nonce: challenge.nonce, expires_at: Proto.from_timestamp(challenge.expires_at)}
    |> Ecto.Changeset.change()
    |> Ecto.Changeset.unique_constraint(:nonce, name: :used_challenges_pkey)
    |> Repo.insert()
    |> case do
      {:ok, _} -> :ok
      {:error, _} -> {:error, :identity_proof_invalid}
    end
  end

  defp mac_input(%V1.IdentityChallengeBody{} = c) do
    [c.nonce, c.host, <<c.expires_at.seconds::64>>, Atom.to_string(c.purpose)]
  end

  defp check_web_origin(%{cert: %V1.DeviceCertBody{kind: :WEB, web_origin: origin}}) do
    if origin in web_origins(), do: :ok, else: {:error, :identity_proof_invalid}
  end

  defp check_web_origin(_device), do: :ok

  # ---- Rosters (sections 3.3, 4.4, 5) -------------------------------------------

  @doc """
  Verifies a signed profile against a verified roster (architecture 6.2,
  "profile imported"): the body names the roster's identity, and its
  signature is by a device of that roster, not revoked. Returns the body.
  """
  def verify_profile(%V1.SignedProfile{body: bytes, signature: %V1.Signature{} = sig}, verified)
      when is_binary(bytes) do
    with {:ok, body} <- decode_profile(bytes),
         true <- body.identity_id == verified.identity_id || {:error, :identity_proof_invalid},
         true <- sig.signer_kind == :DEVICE || {:error, :identity_proof_invalid},
         {:ok, device} <- Map.fetch(verified.devices, sig.signer_id) |> or_invalid(),
         true <-
           is_nil(Map.get(verified.revoked, sig.signer_id)) || {:error, :identity_proof_invalid},
         true <-
           Crypto.verify(bytes, sig.signature, device.cert.public_key.ed25519) ||
             {:error, :identity_proof_invalid} do
      {:ok, body}
    end
  end

  def verify_profile(_, _), do: {:error, :identity_proof_invalid}

  defp decode_profile(bytes) do
    {:ok, V1.ProfileBody.decode(bytes)}
  rescue
    _ -> {:error, :identity_proof_invalid}
  end

  @doc "The roster this instance holds for an identity, verified, or nil."
  def held_roster(identity_id) do
    case Repo.get(HeldRoster, identity_id) do
      nil ->
        nil

      %HeldRoster{roster: bytes} ->
        {:ok, verified} = Roster.verify(V1.SignedRoster.decode(bytes), skip_clock: true)
        verified
    end
  end

  @doc """
  Verifies a roster against the held version and applies the revocation
  policy, then stores it. `context` is `:publish`, `:proof`, or
  `{:revoke, reauth_token, account_id}`.

  A roster that adds revocations is accepted only through the revoke
  context with a valid reauth token, or when root-signed. On acceptance,
  newly revoked devices lose their sessions on this instance and every
  account bound to the identity gets `RosterUpdated` on its log.
  """
  def accept_roster(%V1.SignedRoster{} = signed, context) do
    Tx.transact(fn ->
      with {:ok, body} <- decode_body(signed.body),
           held = lock_held(body.identity_id),
           {:ok, verified} <- verify_against(signed, held),
           :ok <- revocation_policy(verified, context),
           :ok <- store(verified, held),
           :ok <- apply_revocations(verified),
           :ok <- notify_accounts(verified, held),
           :ok <- notify_contacts(verified, held) do
        {:ok, verified}
      end
    end)
  end

  defp decode_body(bytes) do
    {:ok, V1.RosterBody.decode(bytes)}
  rescue
    _ -> {:error, :identity_proof_invalid}
  end

  defp lock_held(identity_id) do
    case Repo.one(from r in HeldRoster, where: r.identity_id == ^identity_id, lock: "FOR UPDATE") do
      nil ->
        nil

      %HeldRoster{roster: bytes} ->
        {:ok, verified} = Roster.verify(V1.SignedRoster.decode(bytes), skip_clock: true)
        verified
    end
  end

  # Same version, same bytes: a no-op that returns the held version.
  defp verify_against(signed, %Roster{signed: %V1.SignedRoster{body: same}} = held)
       when signed.body == same,
       do: {:ok, %{held | new_revocations: []}}

  defp verify_against(signed, held) do
    case Roster.verify(signed, held: held) do
      {:ok, verified} -> {:ok, verified}
      # Older than held: ignored, and the held version goes back to the
      # client so it can adopt it (identity spec 5, "older roster").
      {:error, {:monotonic, :version}} when not is_nil(held) -> {:error, {:conflict, held}}
      {:error, {step, reason}} -> {:error, {:identity_proof_invalid, step, reason}}
    end
  end

  defp revocation_policy(%Roster{new_revocations: []}, _context), do: :ok
  defp revocation_policy(%Roster{root_signed?: true}, _context), do: :ok

  defp revocation_policy(%Roster{} = verified, {:revoke, reauth_token, account_id}) do
    with {:ok, session} <- verify_reauth(reauth_token),
         true <- session.account_id == account_id || {:error, :policy_denied},
         %Account{identity_id: id} when id == verified.identity_id <-
           Repo.get(Account, account_id) do
      :ok
    else
      _ -> {:error, :policy_denied}
    end
  end

  defp revocation_policy(_verified, _context), do: {:error, :policy_denied}

  defp store(verified, held) do
    if held && held.version == verified.version do
      :ok
    else
      bytes = V1.SignedRoster.encode(verified.signed)

      %HeldRoster{
        identity_id: verified.identity_id,
        version: verified.version,
        root_public_key: verified.root_public_key,
        roster: bytes
      }
      |> Repo.insert(
        on_conflict: {:replace, [:version, :root_public_key, :roster, :updated_at]},
        conflict_target: :identity_id
      )
      |> case do
        {:ok, _} -> :ok
        {:error, _} -> {:error, :internal}
      end
    end
  end

  # A new version reaches everyone who shares a personal DM with an account
  # of the identity, so their clients re-judge its MLS leaves (e2ee spec
  # section 4). The same version again is not news.
  defp notify_contacts(%Roster{version: v}, %Roster{version: v}), do: :ok

  defp notify_contacts(%Roster{} = verified, _held) do
    accounts =
      Repo.all(from a in Account, where: a.identity_id == ^verified.identity_id, select: a.id)

    payload =
      {:contact_roster_updated,
       %V1.ContactRosterUpdated{identity_id: verified.identity_id, roster: verified.signed}}

    accounts
    |> Enum.flat_map(&Lumia.Dms.contacts/1)
    |> Enum.uniq()
    |> Enum.each(fn contact -> {:ok, _} = Accounts.append_event(contact, payload) end)

    :ok
  end

  defp apply_revocations(%Roster{new_revocations: []}), do: :ok

  defp apply_revocations(%Roster{} = verified) do
    accounts =
      Repo.all(from a in Account, where: a.identity_id == ^verified.identity_id, select: a.id)

    for account_id <- accounts, device_id_bytes <- verified.new_revocations do
      device_id = Proto.from_uuid(device_id_bytes)
      Auth.revoke_device(account_id, device_id)
      Lumia.Mls.purge_device(account_id, device_id)
    end

    :ok
  end

  # Every new version, additions included, reaches the identity's own
  # accounts: a device that approved a link adds the new leaf to its
  # groups on this signal (e2ee spec section 4).
  defp notify_accounts(%Roster{version: v}, %Roster{version: v}), do: :ok

  defp notify_accounts(%Roster{} = verified, _held) do
    accounts =
      Repo.all(from a in Account, where: a.identity_id == ^verified.identity_id, select: a.id)

    for account_id <- accounts do
      {:ok, _} =
        Accounts.append_event(
          account_id,
          {:roster_updated, %V1.RosterUpdated{roster: verified.signed}}
        )
    end

    :ok
  end

  @doc "Binds an account to an identity. Requires the account to be unbound or already bound to it."
  def link_account(account_id, identity_id) do
    case Repo.get(Account, account_id) do
      %Account{identity_id: nil} = account ->
        account
        |> Ecto.Changeset.change(identity_id: identity_id)
        |> Repo.update()
        |> then(fn {:ok, a} -> {:ok, a} end)

      %Account{identity_id: ^identity_id} = account ->
        {:ok, account}

      _ ->
        {:error, :conflict}
    end
  end

  @doc "The account bound to an identity on this instance, or nil."
  def account_for(identity_id) do
    Repo.one(from a in Account, where: a.identity_id == ^identity_id and is_nil(a.deleted_at))
  end

  # ---- Reauth (section 10) -------------------------------------------------------

  @doc "Issues a reauth token for a session after the caller verified a credential."
  def issue_reauth(%Session{id: session_id}) do
    token = :crypto.strong_rand_bytes(32) |> Base.url_encode64(padding: false)
    now = DateTime.utc_now()
    expires_at = DateTime.add(now, @reauth_ttl_seconds, :second)

    {:ok, _} =
      Repo.insert(%ReauthToken{
        token_hash: :crypto.hash(:sha256, token),
        session_id: session_id,
        expires_at: expires_at,
        created_at: now
      })

    {token, expires_at}
  end

  @doc "Verifies a reauth token and returns its session. Tokens are single use."
  def verify_reauth(token) when is_binary(token) do
    hash = :crypto.hash(:sha256, token)
    now = DateTime.utc_now()

    query =
      from t in ReauthToken,
        where: t.token_hash == ^hash and t.expires_at > ^now,
        select: t.session_id

    case Repo.delete_all(query) do
      {1, [session_id]} ->
        case Repo.get(Session, session_id) do
          %Session{revoked_at: nil} = s -> {:ok, s}
          _ -> {:error, :unauthenticated}
        end

      _ ->
        {:error, :unauthenticated}
    end
  end

  def verify_reauth(_), do: {:error, :unauthenticated}

  # ---- Link bundles (section 6) --------------------------------------------------

  @doc "Stores an encrypted link bundle and returns its single-use code."
  def create_link_bundle(%V1.CreateLinkBundleRequest{} = req) do
    cond do
      byte_size(req.ciphertext) > @link_bundle_max_bytes -> {:error, :payload_too_large}
      byte_size(req.device_id) != 16 -> {:error, :invalid_argument}
      true -> insert_bundle(req, 3)
    end
  end

  defp insert_bundle(_req, 0), do: {:error, :internal}

  defp insert_bundle(req, retries) do
    now = DateTime.utc_now()
    expires_at = DateTime.add(now, @link_ttl_seconds, :second)
    code = for _ <- 1..8, into: "", do: <<Enum.random(@code_alphabet)>>

    %LinkBundle{
      code: code,
      device_id: Proto.from_uuid(req.device_id),
      ciphertext: req.ciphertext,
      sender_ephemeral_public_key: req.sender_ephemeral_public_key,
      nonce: req.nonce,
      expires_at: expires_at,
      created_at: now
    }
    |> Ecto.Changeset.change()
    |> Ecto.Changeset.unique_constraint(:code, name: :link_bundles_pkey)
    |> Repo.insert()
    |> case do
      {:ok, bundle} -> {:ok, bundle}
      {:error, _} -> insert_bundle(req, retries - 1)
    end
  end

  @doc """
  Claims a bundle by code for a device. The first successful fetch deletes
  it; a wrong device id counts an attempt, and the fifth deletes it.
  """
  def claim_link_bundle(code, device_id) do
    # Repo.transaction, not transact: a failed claim is a value, and the
    # attempt counter it wrote must commit.
    {:ok, result} =
      Repo.transaction(fn ->
        now = DateTime.utc_now()

        case Repo.one(
               from b in LinkBundle, where: b.code == ^String.upcase(code), lock: "FOR UPDATE"
             ) do
          nil -> {:error, :not_found}
          %LinkBundle{} = b -> claim(b, device_id, now)
        end
      end)

    result
  end

  defp claim(%LinkBundle{} = b, device_id, now) do
    cond do
      # DateTime structs are compared with DateTime.compare, never <=.
      DateTime.compare(b.expires_at, now) != :gt ->
        Repo.delete(b)
        {:error, :not_found}

      b.device_id == device_id ->
        Repo.delete(b)
        {:ok, b}

      b.attempts + 1 >= @link_attempts ->
        Repo.delete(b)
        {:error, :not_found}

      true ->
        b |> Ecto.Changeset.change(attempts: b.attempts + 1) |> Repo.update()
        {:error, :not_found}
    end
  end

  # ---- Backups (section 8) -------------------------------------------------------

  @doc "Stores an identity backup. The highest version wins."
  def put_backup(identity_id, version, ciphertext, nonce) when version > 0 do
    now = DateTime.utc_now()

    # DO UPDATE ... WHERE existing.version < incoming: the highest version wins.
    on_conflict =
      from b in Backup,
        where: b.version < ^version,
        update: [
          set: [version: ^version, ciphertext: ^ciphertext, nonce: ^nonce, updated_at: ^now]
        ]

    # When the WHERE does not match, the upsert touches no row; Ecto reports
    # that as a stale entry, which here just means a newer version is held.
    %Backup{identity_id: identity_id, version: version, ciphertext: ciphertext, nonce: nonce}
    |> Repo.insert(
      on_conflict: on_conflict,
      conflict_target: :identity_id,
      stale_error_field: :version
    )
    |> case do
      {:ok, _} -> :ok
      {:error, %Ecto.Changeset{errors: [version: _]}} -> :ok
      {:error, _} -> {:error, :internal}
    end
  end

  def put_backup(_, _, _, _), do: {:error, :invalid_argument}

  @doc "The identity backup, or nil."
  def get_backup(identity_id), do: Repo.get(Backup, identity_id)

  # ---- Instance facts ------------------------------------------------------------

  @doc "This instance's host, as bound into challenges."
  def host do
    Application.get_env(:lumia, LumiaWeb.Endpoint, [])
    |> Keyword.get(:url, [])
    |> Keyword.get(:host, "localhost")
  end

  @doc """
  The web origins this instance trusts (identity §7): its own, and the
  flagship origins configured under `:lumia, :identity, :web_origins`. A
  web device's certificate must name one, and only these may call the
  API from a browser (`LumiaWeb.Plugs.Cors`).
  """
  def web_origins do
    [
      own_origin()
      | Application.get_env(:lumia, :identity, []) |> Keyword.get(:web_origins, [])
    ]
  end

  @doc """
  This instance's own web origin: where it serves the web UI from, so a
  self-hosted instance's web origin is itself (architecture 11.4).
  """
  def own_origin, do: LumiaWeb.Endpoint.url()

  defp challenge_key do
    secret = Application.get_env(:lumia, LumiaWeb.Endpoint)[:secret_key_base]
    Plug.Crypto.KeyGenerator.generate(secret, "lumia/challenge/v1")
  end

  defp or_invalid({:ok, v}), do: {:ok, v}
  defp or_invalid(:error), do: {:error, :identity_proof_invalid}
end
