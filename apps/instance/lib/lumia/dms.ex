defmodule Lumia.Dms do
  @moduledoc """
  Personal DMs: `PERSONAL_DM` channels that belong to no server and whose
  ACL is their participant list (architecture 6.4, docs/specs/e2ee.md
  section 1). The instance never sees their content; it keeps the
  participants, the log, and the delivery-service state in `Lumia.Mls`.

  A personal DM between two accounts is unique: `dm_key` is the sha256 of
  the sorted participant ids, and opening it again returns the existing
  channel. Opening one needs a gate (architecture 6.4): a shared personal
  channel already, or a DM request the other side accepted. A personal
  group DM is created by its creator for everyone it is gated with, and is
  never deduplicated.
  """

  import Ecto.Query

  alias Lumia.Accounts
  alias Lumia.Accounts.Account
  alias Lumia.Dms.Participant
  alias Lumia.Dms.Request
  alias Lumia.Events.Channel
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.Tx
  alias Lumia.V1

  @personal_kinds ~w(personal_dm personal_group_dm)

  @doc "Whether a channel kind is end-to-end encrypted."
  def personal_kind?(kind), do: kind in @personal_kinds

  # ---- Requests ----------------------------------------------------------------

  @doc """
  Sends a DM request from one account to another. Returns `{:ok, request}`
  (the existing pending one in either direction when there is one) and
  appends `DmRequestReceived` to the target's log. To oneself is
  `{:error, :invalid_argument}`; an unknown target `{:error,
  :account_not_found}`; a pair that already shares a personal channel
  `{:error, :already_exists}`; a target that answered `blocked` before is
  `{:error, :not_found}`, which discloses nothing.
  """
  def send_request(from_account_id, to_account_id) do
    cond do
      from_account_id == to_account_id -> {:error, :invalid_argument}
      is_nil(Repo.get(Account, to_account_id)) -> {:error, :account_not_found}
      blocked?(from_account_id, to_account_id) -> {:error, :not_found}
      shares_personal_channel?(from_account_id, to_account_id) -> {:error, :already_exists}
      true -> pending_or_create(from_account_id, to_account_id)
    end
  end

  @doc "Whether either account has blocked the other: a DM request between them answered `blocked`."
  def blocked_between?(a, b), do: blocked?(a, b) or blocked?(b, a)

  @doc """
  Whether the account and anyone else in the conversation (a server DM's
  members, architecture 6.4) are blocked either way.
  """
  def blocked_in?(channel_id, account_id) do
    channel_id
    |> Lumia.Channels.member_ids()
    |> Enum.reject(&(&1 == account_id))
    |> Enum.any?(&blocked_between?(account_id, &1))
  end

  defp blocked?(from, to) do
    Repo.exists?(
      from r in Request,
        where: r.from_account_id == ^from and r.to_account_id == ^to and r.state == "blocked"
    )
  end

  defp pending_or_create(from, to) do
    case pending_between(from, to) do
      %Request{} = existing -> {:ok, existing}
      nil -> create_request(from, to)
    end
  end

  defp pending_between(a, b) do
    Repo.one(
      from r in Request,
        where:
          r.state == "pending" and
            ((r.from_account_id == ^a and r.to_account_id == ^b) or
               (r.from_account_id == ^b and r.to_account_id == ^a)),
        limit: 1
    )
  end

  defp create_request(from, to) do
    Tx.transact(fn ->
      with {:ok, request} <-
             Repo.insert(%Request{
               from_account_id: from,
               to_account_id: to,
               created_at: DateTime.utc_now()
             }),
           {:ok, _} <-
             Accounts.append_event(
               to,
               {:dm_request_received, %V1.DmRequestReceived{request: Proto.dm_request(request)}}
             ) do
        {:ok, request}
      end
    end)
  end

  @doc """
  Answers a request as its recipient with `:accepted`, `:declined` or
  `:blocked`. Anyone else sees `{:error, :not_found}`; a request already
  answered is `{:error, :failed_precondition}`. Accepting opens the personal
  DM in the same transaction, after `DmRequestUpdated` is on both logs, so
  a client sees the acceptance before the channel. Returns
  `{:ok, request, channel_or_nil}`.
  """
  def respond(request_id, actor_account_id, state)
      when state in [:accepted, :declined, :blocked] do
    result =
      Tx.transact(fn ->
        with %Request{} = request <- Repo.get(Request, request_id) || {:error, :not_found},
             true <- request.to_account_id == actor_account_id || {:error, :not_found},
             true <- request.state == "pending" || {:error, :failed_precondition},
             {:ok, request} <- answer(request, state),
             :ok <- announce_request(request),
             {:ok, channel} <- open_on_accept(request, state) do
          {:ok, {request, channel}}
        end
      end)

    case result do
      {:ok, {request, channel}} -> {:ok, request, channel}
      error -> error
    end
  end

  defp answer(request, state) do
    request
    |> Ecto.Changeset.change(state: Atom.to_string(state), responded_at: DateTime.utc_now())
    |> Repo.update()
  end

  defp announce_request(request) do
    payload = {:dm_request_updated, %V1.DmRequestUpdated{request: Proto.dm_request(request)}}

    Enum.reduce_while([request.from_account_id, request.to_account_id], :ok, fn id, :ok ->
      case Accounts.append_event(id, payload) do
        {:ok, _} -> {:cont, :ok}
        {:error, reason} -> {:halt, {:error, reason}}
      end
    end)
  end

  defp open_on_accept(request, :accepted) do
    case open_pair(Enum.sort([request.from_account_id, request.to_account_id])) do
      {:ok, channel, _existed} -> {:ok, channel}
      error -> error
    end
  end

  defp open_on_accept(_request, _state), do: {:ok, nil}

  @doc "Whether a request between the two accounts is waiting, in either direction."
  def pending_request?(a, b), do: pending_between(a, b) != nil

  @doc "Whether a request between the two accounts was accepted, in either direction."
  def accepted_request?(a, b) do
    Repo.exists?(
      from r in Request,
        where:
          r.state == "accepted" and
            ((r.from_account_id == ^a and r.to_account_id == ^b) or
               (r.from_account_id == ^b and r.to_account_id == ^a))
    )
  end

  @doc "Requests still waiting for this account's answer, newest first."
  def pending_requests_to(account_id) do
    Repo.all(
      from r in Request,
        where: r.to_account_id == ^account_id and r.state == "pending",
        order_by: [desc: r.created_at]
    )
  end

  # ---- Channels ----------------------------------------------------------------

  @doc """
  Opens a personal DM. With one other account: `{:ok, channel, existed?}`,
  the existing channel when the pair has one, else a new one with
  `ChannelAdded` on both logs; it needs the gate, a shared personal channel
  or an accepted request either way, else `{:error, {:failed_precondition,
  "request"}}`. With two or more others: a new `personal_group_dm` every
  time (`{:ok, channel, false}`), needing the gate between the actor and
  every other participant. No other participants is `{:error,
  :invalid_argument}`.
  """
  def open_personal(actor_account_id, participant_account_ids) do
    ids = Enum.uniq([actor_account_id | participant_account_ids])
    others = ids -- [actor_account_id]

    cond do
      others == [] ->
        {:error, :invalid_argument}

      Enum.any?(others, &(not gated?(actor_account_id, &1))) ->
        {:error, {:failed_precondition, "request"}}

      length(ids) == 2 ->
        open_pair(Enum.sort(ids))

      true ->
        create_group(Enum.sort(ids))
    end
  end

  defp gated?(a, b), do: shares_personal_channel?(a, b) or accepted_request?(a, b)

  defp open_pair(ids) do
    key = dm_key(ids)

    case Repo.get_by(Channel, dm_key: key) do
      %Channel{} = existing -> {:ok, existing, true}
      nil -> create_pair(ids, key)
    end
  end

  defp create_pair(ids, key) do
    result =
      Tx.transact(fn ->
        with {:ok, channel} <- insert_channel("personal_dm", key),
             :ok <- insert_participants(channel.id, ids),
             :ok <- announce(channel, ids) do
          {:ok, channel}
        end
      end)

    case result do
      {:ok, channel} ->
        {:ok, channel, false}

      # A concurrent open won the unique index; theirs is the channel.
      {:error, :dm_key_taken} ->
        {:ok, Repo.get_by!(Channel, dm_key: key), true}

      other ->
        other
    end
  end

  defp create_group(ids) do
    result =
      Tx.transact(fn ->
        with {:ok, channel} <- insert_channel("personal_group_dm", nil),
             :ok <- insert_participants(channel.id, ids),
             :ok <- announce(channel, ids) do
          {:ok, channel}
        end
      end)

    case result do
      {:ok, channel} -> {:ok, channel, false}
      other -> other
    end
  end

  defp insert_channel(kind, key) do
    %Channel{kind: kind, dm_key: key}
    |> Ecto.Changeset.change()
    |> Ecto.Changeset.unique_constraint(:dm_key, name: :channels_dm_key_index)
    |> Repo.insert()
    |> case do
      {:ok, channel} -> {:ok, channel}
      {:error, %Ecto.Changeset{errors: [dm_key: _]}} -> {:error, :dm_key_taken}
      {:error, _} -> {:error, :internal}
    end
  end

  defp insert_participants(channel_id, ids) do
    Enum.reduce_while(ids, :ok, fn account_id, :ok ->
      case Repo.insert(%Participant{channel_id: channel_id, account_id: account_id}) do
        {:ok, _} -> {:cont, Lumia.Reads.start(account_id, channel_id)}
        {:error, _} -> {:halt, {:error, :account_not_found}}
      end
    end)
  end

  defp announce(channel, ids) do
    payload = {:channel_added, %V1.ChannelAdded{channel: Proto.channel(channel)}}

    Enum.reduce_while(ids, :ok, fn account_id, :ok ->
      case Accounts.append_event(account_id, payload) do
        {:ok, _} -> {:cont, :ok}
        {:error, reason} -> {:halt, {:error, reason}}
      end
    end)
  end

  @doc "The `PERSONAL_*` channels the account is in, oldest first."
  def personal_channels(account_id) do
    Repo.all(
      from c in Channel,
        join: p in Participant,
        on: p.channel_id == c.id,
        where: p.account_id == ^account_id,
        order_by: c.inserted_at
    )
  end

  @doc "The participant account ids of a channel, sorted."
  def participants(channel_id) do
    Repo.all(
      from p in Participant,
        where: p.channel_id == ^channel_id,
        order_by: p.account_id,
        select: p.account_id
    )
  end

  @doc "Whether the account is a participant of the channel."
  def participant?(channel_id, account_id) do
    Repo.exists?(
      from p in Participant,
        where: p.channel_id == ^channel_id and p.account_id == ^account_id
    )
  end

  @doc "Of `channel_ids`, the ones the account participates in, as a set; one query."
  def participant_of([], _account_id), do: MapSet.new()

  def participant_of(channel_ids, account_id) do
    Repo.all(
      from p in Participant,
        where: p.channel_id in ^channel_ids and p.account_id == ^account_id,
        select: p.channel_id
    )
    |> MapSet.new()
  end

  @doc """
  Whether two accounts share a `PERSONAL_*` channel: the gate for claiming
  key packages (e2ee spec section 3).
  """
  def shares_personal_channel?(account_a, account_b) do
    Repo.exists?(
      from a in Participant,
        join: b in Participant,
        on: a.channel_id == b.channel_id,
        where: a.account_id == ^account_a and b.account_id == ^account_b
    )
  end

  @doc "Every other account that shares a `PERSONAL_*` channel with the account."
  def contacts(account_id) do
    Repo.all(
      from a in Participant,
        join: b in Participant,
        on: a.channel_id == b.channel_id,
        where: a.account_id == ^account_id and b.account_id != ^account_id,
        distinct: true,
        select: b.account_id
    )
  end

  @doc "The uniqueness key of a participant set: sha256 hex of the sorted ids."
  def dm_key(ids) do
    :crypto.hash(:sha256, ids |> Enum.sort() |> Enum.join(",")) |> Base.encode16(case: :lower)
  end
end
