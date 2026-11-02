defmodule Lumia.Dms do
  @moduledoc """
  Personal DMs: `PERSONAL_DM` channels that belong to no server and whose
  ACL is their participant list (architecture 6.4, docs/specs/e2ee.md
  section 1). The instance never sees their content; it keeps the
  participants, the log, and the delivery-service state in `Lumia.Mls`.

  A personal DM between two accounts is unique: `dm_key` is the sha256 of
  the sorted participant ids, and opening it again returns the existing
  channel.
  """

  import Ecto.Query

  alias Lumia.Accounts
  alias Lumia.Dms.Participant
  alias Lumia.Events.Channel
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.V1

  @personal_kinds ~w(personal_dm personal_group_dm)

  @doc "Whether a channel kind is end-to-end encrypted."
  def personal_kind?(kind), do: kind in @personal_kinds

  @doc """
  Opens the personal DM between the actor and one other account. Returns
  `{:ok, channel, existed?}`: the existing channel when the pair already
  has one, else a new one with `ChannelAdded` on both account logs. More
  than two distinct participants is `{:error, :invalid_argument}`
  (personal group DMs come later).
  """
  def open_personal(actor_account_id, participant_account_ids) do
    ids = Enum.uniq([actor_account_id | participant_account_ids])

    case ids do
      [_, _] -> open_pair(Enum.sort(ids))
      _ -> {:error, :invalid_argument}
    end
  end

  defp open_pair(ids) do
    key = dm_key(ids)

    case Repo.get_by(Channel, dm_key: key) do
      %Channel{} = existing -> {:ok, existing, true}
      nil -> create_pair(ids, key)
    end
  end

  defp create_pair(ids, key) do
    result =
      Repo.transact(fn ->
        with {:ok, channel} <- insert_channel(key),
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

  defp insert_channel(key) do
    %Channel{kind: "personal_dm", dm_key: key}
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
        {:ok, _} -> {:cont, :ok}
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

  @doc "The uniqueness key of a participant set: sha256 hex of the sorted ids."
  def dm_key(ids) do
    :crypto.hash(:sha256, ids |> Enum.sort() |> Enum.join(",")) |> Base.encode16(case: :lower)
  end
end
