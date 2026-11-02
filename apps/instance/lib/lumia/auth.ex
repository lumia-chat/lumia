defmodule Lumia.Auth do
  @moduledoc """
  Sessions and tokens (architecture 6.3, docs/protocol.md section 12).

  A session is one device's authorization on this instance. Tokens are
  opaque random strings; only their SHA-256 is stored. The access token is
  short-lived and validated against the session row on every request, so a
  revoked session fails immediately. The refresh token is long-lived and
  single use: refreshing rotates both, and reuse of an old refresh token
  revokes the whole session.

  Revocation broadcasts on `session:<id>` so a live gateway process closes
  its socket (delivery section 5), and appends `SessionRevoked` to the
  account log.
  """

  import Ecto.Query

  alias Lumia.Accounts
  alias Lumia.Auth.Session
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.Tx
  alias Lumia.V1

  @access_ttl_seconds 15 * 60
  @refresh_ttl_seconds 30 * 24 * 60 * 60

  @doc """
  Creates a session for an account on a device. Returns the session and the
  plaintext tokens, which are shown exactly once.
  """
  def create_session(account_id, device_id, opts \\ []) do
    {access, access_hash} = token()
    {refresh, refresh_hash} = token()
    now = DateTime.utc_now()

    with {:ok, session} <-
           Repo.insert(%Session{
             account_id: account_id,
             device_id: device_id,
             access_hash: access_hash,
             refresh_hash: refresh_hash,
             client: Keyword.get(opts, :client),
             ip: Keyword.get(opts, :ip),
             access_expires_at: DateTime.add(now, @access_ttl_seconds, :second),
             refresh_expires_at: DateTime.add(now, @refresh_ttl_seconds, :second),
             last_seen_at: now
           }) do
      {:ok, session, %{access_token: access, refresh_token: refresh}}
    end
  end

  @doc """
  Verifies an access token. Returns the live session or an error code from
  the protocol's error table: `:unauthenticated`, `:token_expired`,
  `:session_revoked`.
  """
  def verify_access(token) when is_binary(token) do
    now = DateTime.utc_now()

    case Repo.get_by(Session, access_hash: hash(token)) do
      nil ->
        {:error, :unauthenticated}

      %Session{revoked_at: %DateTime{}} ->
        {:error, :session_revoked}

      %Session{} = s ->
        if(expired?(s.access_expires_at, now), do: {:error, :token_expired}, else: {:ok, s})
    end
  end

  def verify_access(_), do: {:error, :unauthenticated}

  @doc """
  Rotates tokens. The old refresh token becomes invalid. Reuse of a refresh
  token that was already rotated revokes the session and answers
  `:session_revoked`, since it means the token leaked (protocol section
  12): the retired hash is kept on the row so the reuse is recognised.
  """
  def refresh(refresh_token) when is_binary(refresh_token) do
    now = DateTime.utc_now()
    presented = hash(refresh_token)

    case Repo.get_by(Session, refresh_hash: presented) do
      nil ->
        reuse(presented)

      %Session{revoked_at: %DateTime{}} ->
        {:error, :session_revoked}

      %Session{} = session ->
        if expired?(session.refresh_expires_at, now),
          do: {:error, :token_expired},
          else: rotate(session, presented, now)
    end
  end

  def refresh(_), do: {:error, :unauthenticated}

  # The token matches no live refresh hash. If it is the one a rotation
  # retired, someone is replaying it: revoke the session it belonged to.
  defp reuse(presented) do
    case Repo.get_by(Session, previous_refresh_hash: presented) do
      %Session{revoked_at: nil, id: id} ->
        _ = revoke(id)
        {:error, :session_revoked}

      %Session{} ->
        {:error, :session_revoked}

      nil ->
        {:error, :unauthenticated}
    end
  end

  # Conditional on the presented hash still being current, so two refreshes
  # racing on one token cannot both succeed: the loser finds the hash
  # retired and is treated as a reuse.
  defp rotate(session, presented, now) do
    {access, access_hash} = token()
    {refresh, refresh_hash} = token()

    query =
      from s in Session,
        where: s.id == ^session.id and s.refresh_hash == ^presented,
        select: s

    update =
      Repo.update_all(query,
        set: [
          access_hash: access_hash,
          refresh_hash: refresh_hash,
          previous_refresh_hash: presented,
          access_expires_at: DateTime.add(now, @access_ttl_seconds, :second),
          refresh_expires_at: DateTime.add(now, @refresh_ttl_seconds, :second),
          last_seen_at: now
        ]
      )

    case update do
      {1, [updated]} -> {:ok, updated, %{access_token: access, refresh_token: refresh}}
      {0, _} -> reuse(presented)
    end
  end

  @doc "Revokes a session: the row, any live socket, and the account log."
  def revoke(session_id) do
    now = DateTime.utc_now()

    query =
      from s in Session,
        where: s.id == ^session_id and is_nil(s.revoked_at),
        select: {s.account_id, s.device_id}

    case Repo.update_all(query, set: [revoked_at: now]) do
      {0, _} ->
        {:error, :not_found}

      {1, [{account_id, device_id}]} ->
        Tx.after_commit(fn ->
          Phoenix.PubSub.broadcast(
            Lumia.PubSub,
            topic(session_id),
            {:session_revoked, session_id}
          )
        end)

        {:ok, _} =
          Accounts.append_event(
            account_id,
            {:session_revoked,
             %V1.SessionRevoked{
               session_id: Proto.uuid(session_id),
               device_id: Proto.uuid(device_id)
             }}
          )

        :ok
    end
  end

  @doc "Revokes every session of a device on this instance (identity spec 4.4)."
  def revoke_device(account_id, device_id) do
    from(s in Session,
      where: s.account_id == ^account_id and s.device_id == ^device_id and is_nil(s.revoked_at),
      select: s.id
    )
    |> Repo.all()
    |> Enum.each(&revoke/1)
  end

  @doc "Live sessions for an account, newest first."
  def list_sessions(account_id) do
    Repo.all(
      from s in Session,
        where: s.account_id == ^account_id and is_nil(s.revoked_at),
        order_by: [desc: s.inserted_at]
    )
  end

  @doc "The PubSub topic a gateway process subscribes to for revocation."
  def topic(session_id), do: "session:" <> session_id

  @doc "Access token lifetime, for Welcome and documentation."
  def access_ttl_seconds, do: @access_ttl_seconds

  # DateTime structs must not be compared with > or <; that is structural.
  defp expired?(expires_at, now), do: DateTime.compare(expires_at, now) != :gt

  defp token do
    plaintext = :crypto.strong_rand_bytes(32) |> Base.url_encode64(padding: false)
    {plaintext, hash(plaintext)}
  end

  defp hash(plaintext), do: :crypto.hash(:sha256, plaintext)
end
