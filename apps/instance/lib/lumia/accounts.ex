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
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.Schema
  alias Lumia.V1

  @doc "Creates an account. Spike-level; auth flows come with the identity endpoints."
  def create_account(attrs) do
    %Account{}
    |> Ecto.Changeset.cast(attrs, [
      :identity_id,
      :kind,
      :handle,
      :display_name,
      :email,
      :instance_role
    ])
    |> Ecto.Changeset.validate_required([:handle])
    |> Ecto.Changeset.validate_inclusion(:kind, ~w(human bot))
    |> Ecto.Changeset.validate_inclusion(:instance_role, ~w(none moderator admin owner))
    |> Ecto.Changeset.unique_constraint(:handle, name: :accounts_handle_index)
    |> Ecto.Changeset.unique_constraint(:email, name: :accounts_email_index)
    |> Repo.insert()
  end

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
      Repo.transact(fn ->
        now = DateTime.utc_now()
        head_at = DateTime.truncate(now, :second)

        lock = from(a in Account, where: a.id == ^account_id, select: a.last_seq)

        case Repo.update_all(lock, inc: [last_seq: 1], set: [last_seq_at: head_at]) do
          {0, _} -> {:error, :account_not_found}
          {1, [seq]} -> write(account_id, seq, payload, now)
        end
      end)

    with {:ok, {event, bytes}} <- result do
      if Keyword.get(opts, :broadcast, true), do: broadcast(account_id, event, bytes)
      {:ok, %{account_id: account_id, seq: event.seq, event_id: Proto.from_uuid(event.event_id)}}
    end
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
