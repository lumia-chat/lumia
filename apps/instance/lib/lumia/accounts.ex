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
  alias Lumia.Accounts.HandleHold
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

  # A handle left by a rename is held for its account this long.
  @handle_hold_days 30
  @display_name_max 64

  @doc """
  Whether `handle` is one an account may be made with or renamed to: 2 to
  32 of the letters a to z, digits, `_`, `.` and `-`, beginning with a
  letter, a digit or `_`. Handles are unique without regard to case.
  """
  def valid_handle?(handle) when is_binary(handle),
    do: Regex.match?(~r/\A[A-Za-z0-9_][A-Za-z0-9_.-]{1,31}\z/, handle)

  def valid_handle?(_), do: false

  @doc """
  Changes the account's own handle and display name (`PATCH /account`);
  `changes` has `:handle`, `:display_name`, both or neither, and a key
  left out is left as it is. A display name of nothing clears it.

  The handle it leaves is held for it for #{@handle_hold_days} days: nobody
  else can take it, and it can go back. `{:error, :already_exists}` for a
  handle another account has or holds, `{:error, :invalid_argument}` for
  one that is not a handle or a name that is too long. `ProfileUpdated`
  goes on the account's log.

  The display name set here stands until a signed profile sets another
  (`put_profile/2`), and the other way round: the later of the two is the
  account's name on this instance (architecture 6.2, "Names").
  """
  def update_account(%Account{} = account, changes) when is_map(changes) do
    handle = changes |> Map.get(:handle, account.handle) |> to_string() |> String.trim()
    name = new_name(account, changes)

    cond do
      not acceptable?(account, handle, name) ->
        {:error, :invalid_argument}

      handle == account.handle and name == account.display_name ->
        {:ok, account}

      true ->
        renamed? = String.downcase(handle) != String.downcase(account.handle)
        Tx.transact(fn -> store_account(account, handle, name, renamed?) end)
    end
  end

  defp new_name(account, changes) do
    case Map.fetch(changes, :display_name) do
      {:ok, name} -> name |> to_string() |> String.trim() |> blank_to_nil()
      :error -> account.display_name
    end
  end

  # Only a handle being changed is held to the rule: an account made before
  # there was one keeps what it has, and can still change its name.
  defp acceptable?(account, handle, name) do
    (handle == account.handle or valid_handle?(handle)) and
      (is_nil(name) or String.length(name) <= @display_name_max)
  end

  defp store_account(account, handle, name, renamed?) do
    with :ok <- if(renamed?, do: free_for(handle, account.id), else: :ok),
         {:ok, updated} <-
           account
           |> Ecto.Changeset.change(handle: handle, display_name: name)
           |> Ecto.Changeset.unique_constraint(:handle, name: :accounts_handle_index)
           |> Repo.update()
           |> taken_as_exists(),
         :ok <- if(renamed?, do: hold(account), else: :ok),
         {:ok, _} <-
           append_event(
             account.id,
             {:profile_updated, %V1.ProfileUpdated{account: Lumia.Proto.account(updated)}}
           ) do
      {:ok, updated}
    end
  end

  # Whether nobody else holds `handle` from a rename of their own. A hold
  # that has run out, or the account's own, is dropped on the way.
  @avatar_path "/api/v1/avatars/"

  @doc """
  Makes `upload` (a picture's bytes, as chosen) the account's avatar: the
  instance turns it into a square of its own encoding (`Lumia.Media`),
  keeps that as a blob (`Lumia.Blobs`) and names it in the account's
  `avatar_url` as a path on this instance, `#{@avatar_path}<address>`.
  The picture it replaces is removed, and `ProfileUpdated` goes on the
  account's log.

  `{:error, {:failed_precondition, "blobs"}}` on an instance that keeps
  no blobs; `{:error, reason}` from `Lumia.Media.avatar/1` for a file
  that is not a picture it takes.
  """
  def set_avatar(%Account{} = account, upload) when is_binary(upload) do
    with true <- Lumia.Blobs.enabled?() || {:error, {:failed_precondition, "blobs"}},
         {:ok, {media_type, bytes}} <- Lumia.Media.avatar(upload),
         {:ok, blob} <- Lumia.Blobs.put(bytes, "avatar", media_type, account.id),
         {:ok, updated} <- put_avatar_url(account, @avatar_path <> Lumia.Blobs.name(blob.address)) do
      {:ok, updated}
    else
      {:error, :unavailable} -> {:error, {:failed_precondition, "blobs"}}
      other -> other
    end
  end

  @doc "Takes the account's avatar away; fine when it has none."
  def remove_avatar(%Account{avatar_url: nil} = account), do: {:ok, account}
  def remove_avatar(%Account{} = account), do: put_avatar_url(account, nil)

  @doc "The address of the blob an `avatar_url` names on this instance, or nil for another's."
  def avatar_address(@avatar_path <> name), do: Lumia.Blobs.address(name)
  def avatar_address(_), do: nil

  defp put_avatar_url(account, url) do
    result =
      Tx.transact(fn ->
        with {:ok, updated} <- account |> Ecto.Changeset.change(avatar_url: url) |> Repo.update(),
             {:ok, _} <-
               append_event(
                 account.id,
                 {:profile_updated, %V1.ProfileUpdated{account: Lumia.Proto.account(updated)}}
               ) do
          {:ok, updated}
        end
      end)

    with {:ok, updated} <- result do
      drop_avatar(account.avatar_url, updated.avatar_url)
      {:ok, updated}
    end
  end

  # The picture an account no longer names is nobody's: its blob goes.
  defp drop_avatar(same, same), do: :ok

  defp drop_avatar(old, _new) do
    if address = avatar_address(old), do: Lumia.Blobs.delete(address)
    :ok
  end

  defp free_for(handle, account_id) do
    now = DateTime.utc_now()

    holds =
      from h in HandleHold, where: fragment("lower(?)", h.handle) == ^String.downcase(handle)

    spent =
      if account_id,
        do: where(holds, [h], h.account_id == ^account_id or h.until <= ^now),
        else: where(holds, [h], h.until <= ^now)

    Repo.delete_all(spent)
    if Repo.exists?(holds), do: {:error, :already_exists}, else: :ok
  end

  defp hold(%Account{id: id, handle: left}) do
    until = DateTime.add(DateTime.utc_now(), @handle_hold_days * 86_400, :second)

    Repo.insert!(%HandleHold{account_id: id, handle: left, until: until},
      on_conflict: {:replace, [:handle, :until]},
      conflict_target: :account_id
    )

    :ok
  end

  defp taken_as_exists({:error, %Ecto.Changeset{errors: errors}} = error) do
    taken? =
      match?({_, opts} when is_list(opts), errors[:handle]) and
        elem(errors[:handle], 1)[:constraint] == :unique

    if taken?, do: {:error, :already_exists}, else: error
  end

  defp taken_as_exists(other), do: other

  @doc """
  Creates an account. A handle that is not one (`valid_handle?/1`) is a
  changeset error; one another account has, or holds from a rename, is
  `{:error, :already_exists}`.
  """
  def create_account(attrs) do
    handle = attrs[:handle] || attrs["handle"]

    with true <- is_binary(handle) || {:error, :invalid_argument},
         :ok <- free_for(handle, nil) do
      attrs |> account_changeset() |> Repo.insert() |> taken_as_exists()
    end
  end

  defp account_changeset(attrs) do
    %Account{}
    |> Ecto.Changeset.cast(attrs, [
      :identity_id,
      :kind,
      :handle,
      :display_name,
      :avatar_url,
      :pronouns,
      :bio,
      :email,
      :instance_role,
      :profile
    ])
    |> Ecto.Changeset.validate_required([:handle])
    |> Ecto.Changeset.validate_change(:handle, fn :handle, handle ->
      if valid_handle?(handle),
        do: [],
        else: [handle: "2 to 32 letters, digits, _, . or -, not beginning with . or -"]
    end)
    |> Ecto.Changeset.validate_inclusion(:kind, ~w(human bot))
    |> Ecto.Changeset.validate_inclusion(:instance_role, ~w(none moderator admin owner))
    |> Ecto.Changeset.unique_constraint(:handle, name: :accounts_handle_index)
    |> Ecto.Changeset.unique_constraint(:email, name: :accounts_email_index)
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

  # The avatar is the account's own (`set_avatar/2`) unless the profile
  # names one: a profile signed before the picture was set, or by a device
  # that knows of none, does not take it away.
  defp store_profile(account, body, signed) do
    with {:ok, updated} <-
           account
           |> Ecto.Changeset.change(
             display_name: blank_to_nil(body.display_name),
             avatar_url: blank_to_nil(body.avatar_url) || account.avatar_url,
             pronouns: blank_to_nil(body.pronouns),
             bio: blank_to_nil(body.bio),
             profile: V1.SignedProfile.encode(signed)
           )
           |> Repo.update(),
         {:ok, _} <-
           append_event(
             account.id,
             {:profile_updated, %V1.ProfileUpdated{account: Lumia.Proto.account(updated)}}
           ) do
      Tx.after_commit(fn -> drop_avatar(account.avatar_url, updated.avatar_url) end)
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
  def check_second_factor(account, totp_code, passkey \\ nil) do
    with :ok <- check_totp(account, totp_code) do
      check_passkey(account, passkey)
    end
  end

  defp check_totp(%Account{totp_enabled_at: nil}, _code), do: :ok
  defp check_totp(%Account{}, ""), do: {:error, {:failed_precondition, :mfa}}

  defp check_totp(%Account{totp_secret: secret}, code) do
    if Totp.valid?(secret, code), do: :ok, else: {:error, :unauthenticated}
  end

  # The passkey as the second factor (protocol 6.1's details.mfa): the
  # assertion made for an assert challenge, by one of this account's keys.
  defp check_passkey(%Account{passkey_required_at: nil}, _assertion), do: :ok
  defp check_passkey(%Account{}, nil), do: {:error, {:failed_precondition, :passkey}}

  defp check_passkey(%Account{id: id}, {challenge_id, credential_json}) do
    case Lumia.Auth.Passkeys.verify_assertion(challenge_id, credential_json) do
      {:ok, ^id} -> :ok
      _ -> {:error, :unauthenticated}
    end
  end

  @doc """
  Whether a password sign-in must also present one of the account's
  passkeys. Turning it on needs a passkey to exist (`{:error,
  {:failed_precondition, "no_passkey"}}` otherwise).
  """
  def set_passkey_required(account_id, required?) do
    account = Repo.get!(Account, account_id)

    cond do
      required? and Lumia.Auth.Passkeys.passkeys(account_id) == [] ->
        {:error, {:failed_precondition, "no_passkey"}}

      required? ->
        account
        |> Ecto.Changeset.change(
          passkey_required_at: account.passkey_required_at || DateTime.utc_now()
        )
        |> Repo.update()

      true ->
        account |> Ecto.Changeset.change(passkey_required_at: nil) |> Repo.update()
    end
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
