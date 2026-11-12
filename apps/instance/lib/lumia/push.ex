defmodule Lumia.Push do
  @moduledoc """
  Content-free push notifications (architecture 15.5, deployment spec):
  for a personal DM message or an account mention in a server channel,
  every device of the recipients that has no live session gets "New
  message" through its subscription. The decision is made after the
  event's commit and carried out by `Lumia.Workers.Push`, so a slow push
  service never holds a send.

  Liveness is `LumiaWeb.Presence`: a session tracks its device on join,
  cluster-wide, and a device present anywhere is not pushed.
  """

  import Ecto.Query

  alias Lumia.Dms
  alias Lumia.Events.Channel
  alias Lumia.Push.Subscription
  alias Lumia.Repo
  alias Lumia.V1
  alias Lumia.Workers

  @notice %{"title" => "Lumia", "body" => "New message"}

  @doc "The generic notice every push carries."
  def notice, do: @notice

  @doc "Registers (or replaces) a device's subscription of a kind."
  def put(account_id, device_id, %V1.PushSubscription{} = sub) do
    attrs = %{
      account_id: account_id,
      device_id: device_id,
      kind: kind_name(sub.kind),
      endpoint: sub.endpoint,
      p256dh: blank_to_nil(sub.p256dh),
      auth: blank_to_nil(sub.auth)
    }

    %Subscription{}
    |> Subscription.changeset(attrs)
    |> Repo.insert(
      on_conflict: {:replace, [:account_id, :endpoint, :p256dh, :auth, :updated_at]},
      conflict_target: [:device_id, :kind]
    )
    |> case do
      {:ok, _} -> :ok
      {:error, _changeset} -> {:error, :invalid_argument}
    end
  end

  @doc "Removes a device's subscription of a kind."
  def delete(device_id, kind) when is_binary(kind) do
    Repo.delete_all(from s in Subscription, where: s.device_id == ^device_id and s.kind == ^kind)
    :ok
  end

  @doc """
  Removes every subscription a device holds for an account: what
  `Lumia.Auth.revoke/1` does when the device's last session of the account
  ends, however it ended (signing out, being signed out from elsewhere, the
  device revoked from the identity).
  """
  def forget(account_id, device_id) do
    Repo.delete_all(
      from s in Subscription, where: s.account_id == ^account_id and s.device_id == ^device_id
    )

    :ok
  end

  @doc "The subscriptions of these accounts, by device."
  def for_accounts(account_ids) do
    Repo.all(from s in Subscription, where: s.account_id in ^account_ids)
  end

  @doc "Whether a device has a live session on any node."
  def online?(device_id), do: Map.has_key?(LumiaWeb.Presence.list("devices"), device_id)

  @doc """
  Called after an event committed: enqueues the push job when the event
  is a personal DM application message or a server message that mentions
  an account. Cheap; the job does the lookups.
  """
  def after_commit(channel_id, %V1.Event{} = event) do
    if pushable?(event) do
      %{channel_id: channel_id, seq: event.seq}
      |> Workers.Push.new()
      |> Oban.insert()
    end

    :ok
  end

  defp pushable?(%V1.Event{payload: {:encrypted, %V1.EncryptedPayload{kind: :APPLICATION}}}),
    do: true

  defp pushable?(%V1.Event{payload: {:message_created, %V1.MessageCreated{message: message}}}),
    do: Enum.any?(message.mentions, &match?(%V1.Mention{target: {:account_id, _}}, &1))

  defp pushable?(_), do: false

  @doc """
  Who to notify for the event at `seq` in `channel_id`, as account ids:
  the other participants of a personal channel, or the accounts mentioned
  in a server message. Never the actor.
  """
  def recipients(channel_id, %V1.Event{} = event) do
    actor = Lumia.Proto.from_uuid(event.actor_account_id)

    case {Repo.get(Channel, channel_id), event.payload} do
      {%Channel{kind: kind}, {:encrypted, _}} ->
        if Dms.personal_kind?(kind), do: Dms.participants(channel_id) -- [actor], else: []

      {%Channel{}, {:message_created, %V1.MessageCreated{message: message}}} ->
        message.mentions
        |> Enum.flat_map(fn
          %V1.Mention{target: {:account_id, id}} -> [Lumia.Proto.from_uuid(id)]
          _ -> []
        end)
        |> Enum.reject(&(&1 == actor))
        |> Enum.uniq()

      _ ->
        []
    end
  end

  @doc "Sends the notice to every offline device of the recipients; returns how many went."
  def deliver(channel_id, %V1.Event{} = event) do
    recipients(channel_id, event)
    |> for_accounts()
    |> Enum.reject(&online?(&1.device_id))
    |> Enum.reduce(0, fn sub, sent ->
      case Lumia.Push.Sender.send(sub, @notice) do
        :ok ->
          sent + 1

        {:error, :gone} ->
          # The push service forgot the subscription; so do we.
          delete(sub.device_id, sub.kind)
          sent

        {:error, _} ->
          sent
      end
    end)
  end

  defp kind_name(kind) when is_atom(kind), do: kind |> Atom.to_string() |> String.downcase()
  defp kind_name(kind) when is_integer(kind), do: kind_name(V1.PushKind.key(kind))

  defp blank_to_nil(""), do: nil
  defp blank_to_nil(value), do: value
end
