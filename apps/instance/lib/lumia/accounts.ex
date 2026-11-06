defmodule Lumia.Accounts do
  @moduledoc """
  Accounts and the account log.

  The account log (docs/specs/delivery.md section 2) is the second kind of
  log: one per account, carrying what a client cannot learn from channel
  logs it does not yet know about. Only the server appends to it, so there
  are no commands and no idempotency table; otherwise it is the same shape
  as the channel log and the same rules apply (dense `seq`, commit then
  broadcast, reconciliation by `last_seq_at`). A stored payload is the
  serialized `Lumia.V1.AccountEvent` that goes on the wire.
  """

  import Ecto.Query

  alias Lumia.Accounts.Account
  alias Lumia.Accounts.AccountEvent
  alias Lumia.Accounts.AccountEventPayload
  alias Lumia.Auth.Totp
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.Schema
  alias Lumia.Tx
  alias Lumia.V1

  @doc """
  Whether `viewer_id` may see `account_id`'s public profile: itself, an
  account it shares a server or a `PERSONAL_*` channel with, or one with
  a request waiting between them (the recipient needs the sender's name
  to answer; architecture 6.4 shows nothing more than that).
  """
  def visible_to?(viewer_id, account_id) do
    viewer_id == account_id or shares_server?(viewer_id, account_id) or
      Lumia.Dms.shares_personal_channel?(viewer_id, account_id) or
      Lumia.Dms.pending_request?(viewer_id, account_id)
  end

  defp shares_server?(a, b) do
    Repo.exists?(
      from m in Lumia.Servers.Member,
        join: n in Lumia.Servers.Member,
        on: m.server_id == n.server_id,
        where: m.account_id == ^a and n.account_id == ^b
    )
  end

  @doc "Creates an account. Spike-level; auth flows come with the identity endpoints."
  def create_account(attrs) do
    %Account{}
    |> Ecto.Changeset.cast(attrs, [
      :identity_id,
      :kind,
      :handle,
      :display_name,
      :avatar_url,
      :email,
      :instance_role,
      :profile
    ])
    |> Ecto.Changeset.validate_required([:handle])
    |> Ecto.Changeset.validate_inclusion(:kind, ~w(human bot))
    |> Ecto.Changeset.validate_inclusion(:instance_role, ~w(none moderator admin owner))
    |> Ecto.Changeset.unique_constraint(:handle, name: :accounts_handle_index)
    |> Ecto.Changeset.unique_constraint(:email, name: :accounts_email_index)
    |> Repo.insert()
  end

  @doc """
  Takes a new signed profile for an identity-bound account: verified against
  the identity's held roster, newer than the one held, then the account's
  display name and avatar, with `ProfileUpdated` on its log.
  """
  def put_profile(%Account{identity_id: identity_id} = account, %V1.SignedProfile{} = signed)
      when is_binary(identity_id) do
    with verified when not is_nil(verified) <- Lumia.Identity.held_roster(identity_id),
         {:ok, body} <- Lumia.Identity.verify_profile(signed, verified),
         :ok <- newer_than_held(account, body) do
      Tx.transact(fn -> store_profile(account, body, signed) end)
    else
      nil -> {:error, :identity_proof_invalid}
      other -> other
    end
  end

  def put_profile(_, _), do: {:error, :failed_precondition}

  defp store_profile(account, body, signed) do
    with {:ok, updated} <-
           account
           |> Ecto.Changeset.change(
             display_name: blank_to_nil(body.display_name),
             avatar_url: blank_to_nil(body.avatar_url),
             profile: V1.SignedProfile.encode(signed)
           )
           |> Repo.update(),
         {:ok, _} <-
           append_event(
             account.id,
             {:profile_updated, %V1.ProfileUpdated{account: Lumia.Proto.account(updated)}}
           ) do
      {:ok, updated}
    end
  end

  defp newer_than_held(%Account{profile: nil}, _body), do: :ok

  defp newer_than_held(%Account{profile: bytes}, body) do
    held = V1.SignedProfile.decode(bytes).body |> V1.ProfileBody.decode()
    if body.version > held.version, do: :ok, else: {:error, :conflict}
  end

  defp blank_to_nil(""), do: nil
  defp blank_to_nil(v), do: v

  @doc "Creates an account with a password (Argon2id)."
  def register(%{password: password} = attrs) when is_binary(password) do
    with :ok <- validate_password(password),
         {:ok, account} <- create_account(Map.delete(attrs, :password)) do
      account
      |> Ecto.Changeset.change(password_hash: Argon2.hash_pwd_salt(password))
      |> Repo.update()
    end
  end

  @doc "Verifies a password for a handle or email. Constant-time on miss."
  def authenticate(handle_or_email, password)
      when is_binary(handle_or_email) and is_binary(password) do
    needle = String.downcase(handle_or_email)

    account =
      Repo.one(
        from a in Account,
          where:
            (fragment("lower(?)", a.handle) == ^needle or fragment("lower(?)", a.email) == ^needle) and
              is_nil(a.deleted_at)
      )

    cond do
      is_nil(account) or is_nil(account.password_hash) ->
        Argon2.no_user_verify()
        {:error, :unauthenticated}

      Argon2.verify_pass(password, account.password_hash) ->
        {:ok, account}

      true ->
        {:error, :unauthenticated}
    end
  end

  @doc """
  Starts TOTP enrolment: a pending secret replaces any unconfirmed one.
  Returns `{:ok, %{secret: base32, otpauth_uri: uri}}`; `{:error,
  :already_exists}` when TOTP is already enabled.
  """
  def enroll_totp(account_id) do
    case Repo.get(Account, account_id) do
      nil ->
        {:error, :account_not_found}

      %Account{totp_enabled_at: %DateTime{}} ->
        {:error, :already_exists}

      %Account{} = account ->
        secret = Totp.generate_secret()

        with {:ok, _} <-
               account |> Ecto.Changeset.change(totp_secret: secret) |> Repo.update() do
          {:ok,
           %{
             secret: Totp.encode(secret),
             otpauth_uri: Totp.otpauth_uri(secret, account.handle, Lumia.Identity.host())
           }}
        end
    end
  end

  @doc "Enables TOTP with the first valid code against the pending secret."
  def confirm_totp(account_id, code) do
    case Repo.get(Account, account_id) do
      %Account{totp_secret: secret, totp_enabled_at: nil} = account when is_binary(secret) ->
        if Totp.valid?(secret, code),
          do: enable_totp(account),
          else: {:error, :unauthenticated}

      %Account{totp_enabled_at: %DateTime{}} ->
        {:error, :already_exists}

      _ ->
        {:error, :failed_precondition}
    end
  end

  defp enable_totp(account) do
    account
    |> Ecto.Changeset.change(totp_enabled_at: DateTime.utc_now())
    |> Repo.update()
    |> case do
      {:ok, _} -> :ok
      {:error, _} -> {:error, :internal}
    end
  end

  @doc """
  The second factor after a password: `:ok` when none is enabled or the
  code is valid; `{:error, {:failed_precondition, :mfa}}` when one is
  required and no code was given; `{:error, :unauthenticated}` for a bad code.
  """
  def check_second_factor(%Account{totp_enabled_at: nil}, _code), do: :ok
  def check_second_factor(%Account{}, ""), do: {:error, {:failed_precondition, :mfa}}

  def check_second_factor(%Account{totp_secret: secret}, code) do
    if Totp.valid?(secret, code), do: :ok, else: {:error, :unauthenticated}
  end

  defp validate_password(password) do
    if String.length(password) >= 12 and byte_size(password) <= 1024,
      do: :ok,
      else: {:error, :invalid_argument}
  end

  @doc """
  Appends an event to an account's log inside one transaction: lock the
  account row by incrementing `last_seq`, insert the event and its payload,
  commit, then broadcast on `account:<id>`. `payload` is a
  `Lumia.V1.AccountEvent` payload tuple such as
  `{:membership_added, %Lumia.V1.MembershipAdded{}}`.
  """
  def append_event(account_id, {tag, _} = payload, opts \\ []) when is_atom(tag) do
    result =
      Tx.transact(fn ->
        now = DateTime.utc_now()
        head_at = DateTime.truncate(now, :second)

        lock = from(a in Account, where: a.id == ^account_id, select: a.last_seq)

        case Repo.update_all(lock, inc: [last_seq: 1], set: [last_seq_at: head_at]) do
          {0, _} -> {:error, :account_not_found}
          {1, [seq]} -> write(account_id, seq, payload, now)
        end
      end)

    with {:ok, {event, bytes}} <- result do
      broadcast_after_commit(account_id, event, bytes, opts)
      {:ok, %{account_id: account_id, seq: event.seq, event_id: Proto.from_uuid(event.event_id)}}
    end
  end

  # Deferred until the outermost transaction commits (see Lumia.Tx).
  defp broadcast_after_commit(account_id, event, bytes, opts) do
    if Keyword.get(opts, :broadcast, true),
      do: Tx.after_commit(fn -> broadcast(account_id, event, bytes) end)
  end

  defp write(account_id, seq, payload, now) do
    event = %V1.AccountEvent{
      event_id: Proto.uuid(Schema.generate_id()),
      account_id: Proto.uuid(account_id),
      seq: seq,
      created_at: Proto.timestamp(now),
      payload: payload
    }

    bytes = V1.AccountEvent.encode(event)

    with {:ok, _} <-
           Repo.insert(%AccountEvent{
             account_id: account_id,
             seq: seq,
             id: Proto.from_uuid(event.event_id),
             kind: Proto.kind(payload),
             created_at: now
           }),
         {:ok, _} <-
           Repo.insert(%AccountEventPayload{account_id: account_id, seq: seq, payload: bytes}) do
      {:ok, {event, bytes}}
    end
  end

  @doc "Account events after `after_seq`, in order, as `Lumia.V1.AccountEvent` structs."
  def events_after(account_id, after_seq, limit \\ 200) do
    from(e in AccountEvent,
      left_join: p in AccountEventPayload,
      on: p.account_id == e.account_id and p.seq == e.seq,
      where: e.account_id == ^account_id and e.seq > ^after_seq,
      order_by: [asc: e.seq],
      limit: ^limit,
      select: {e, p.payload}
    )
    |> Repo.all()
    |> Enum.map(fn
      {_row, bytes} when is_binary(bytes) ->
        V1.AccountEvent.decode(bytes)

      {row, nil} ->
        %V1.AccountEvent{
          event_id: Proto.uuid(row.id),
          account_id: Proto.uuid(account_id),
          seq: row.seq,
          created_at: Proto.timestamp(row.created_at)
        }
    end)
  end

  @doc "Event kinds on an account's log, in order. For tests."
  def event_kinds(account_id) do
    Repo.all(
      from e in AccountEvent, where: e.account_id == ^account_id, order_by: e.seq, select: e.kind
    )
  end

  @doc "The current head of an account log, or nil."
  def head(account_id) do
    Repo.one(from a in Account, where: a.id == ^account_id, select: a.last_seq)
  end

  @doc "Accounts whose log head moved at or after `since`. The reconciliation query."
  def changed_since(%DateTime{} = since) do
    since = DateTime.truncate(since, :second)

    Repo.all(
      from a in Account,
        where: a.last_seq_at >= ^since,
        select: {a.id, a.last_seq}
    )
  end

  @doc "The PubSub topic for an account log."
  def topic(account_id), do: "account:" <> account_id

  @doc "Broadcasts a committed account event: the struct and its serialized bytes."
  def broadcast(account_id, %V1.AccountEvent{} = event, bytes) when is_binary(bytes) do
    Phoenix.PubSub.broadcast(Lumia.PubSub, topic(account_id), {:account_event, event, bytes})
  end
end
