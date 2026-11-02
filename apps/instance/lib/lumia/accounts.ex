defmodule Lumia.Accounts do
  @moduledoc """
  Accounts and the account log.

  The account log (docs/specs/delivery.md section 2) is the second kind of
  log: one per account, carrying what a client cannot learn from channel
  logs it does not yet know about. Only the server appends to it, so there
  are no commands and no idempotency table; otherwise it is the same shape
  as the channel log and the same rules apply (dense `seq`, commit then
  broadcast, reconciliation by `last_seq_at`).
  """

  import Ecto.Query

  alias Lumia.Accounts.Account
  alias Lumia.Accounts.AccountEvent
  alias Lumia.Accounts.AccountEventPayload
  alias Lumia.Repo
  alias Lumia.Schema

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

  @doc """
  Appends an event to an account's log inside one transaction: lock the
  account row by incrementing `last_seq`, insert the event and its payload,
  commit. Broadcast is the caller's job and happens after this returns.
  """
  def append_event(account_id, kind, payload) when is_binary(kind) and is_map(payload) do
    Repo.transact(fn ->
      now = DateTime.utc_now()
      head_at = DateTime.truncate(now, :second)

      lock = from(a in Account, where: a.id == ^account_id, select: a.last_seq)

      case Repo.update_all(lock, inc: [last_seq: 1], set: [last_seq_at: head_at]) do
        {0, _} -> {:error, :account_not_found}
        {1, [seq]} -> write(account_id, seq, kind, payload, now)
      end
    end)
  end

  defp write(account_id, seq, kind, payload, now) do
    event_id = Schema.generate_id()

    with {:ok, _} <-
           Repo.insert(%AccountEvent{
             account_id: account_id,
             seq: seq,
             id: event_id,
             kind: kind,
             created_at: now
           }),
         {:ok, _} <-
           Repo.insert(%AccountEventPayload{account_id: account_id, seq: seq, payload: payload}) do
      {:ok, %{account_id: account_id, seq: seq, event_id: event_id}}
    end
  end

  @doc "Account events after `after_seq`, in order."
  def events_after(account_id, after_seq, limit \\ 200) do
    Repo.all(
      from e in AccountEvent,
        where: e.account_id == ^account_id and e.seq > ^after_seq,
        order_by: [asc: e.seq],
        limit: ^limit
    )
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
end
