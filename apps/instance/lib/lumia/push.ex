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
  is a personal DM application message, a server message that mentions
  an account, or a message in a server DM. Cheap; the job does the
  lookups.
  """
  def after_commit(channel_id, %V1.Event{} = event) do
    if pushable?(event) or server_dm_message?(channel_id, event) do
      %{channel_id: channel_id, seq: event.seq}
      |> Workers.Push.new()
      |> Oban.insert()
    end

    :ok
  end

  defp pushable?(%V1.Event{payload: {:encrypted, %V1.EncryptedPayload{kind: :APPLICATION}}}),
    do: true

  defp pushable?(%V1.Event{payload: {:message_created, %V1.MessageCreated{message: message}}}),
    do: message.mentions != [] or message.reply_to != nil

  defp pushable?(_), do: false

  defp server_dm_message?(channel_id, %V1.Event{
         payload: {:message_created, %V1.MessageCreated{message: %V1.Message{notice: nil}}}
       }) do
    Repo.exists?(from c in Channel, where: c.id == ^channel_id and c.kind == "server_dm")
  end

  defp server_dm_message?(_channel_id, _event), do: false

  @doc """
  Who to notify for the event at `seq` in `channel_id`, as account ids:
  the other participants of a personal channel, the other member of a
  server DM, or in a server message the accounts it mentions, everyone if
  it mentions everyone, and the author of the message it quotes, of those
  in the channel who see it (architecture 6.8: someone outside it cannot
  read what mentions them). Never the actor.
  """
  def recipients(channel_id, %V1.Event{} = event) do
    actor = Lumia.Proto.from_uuid(event.actor_account_id)

    case {Repo.get(Channel, channel_id), event.payload} do
      {%Channel{kind: kind}, {:encrypted, _}} ->
        if Dms.personal_kind?(kind), do: Dms.participants(channel_id) -- [actor], else: []

      {%Channel{kind: "server_dm"}, {:message_created, %V1.MessageCreated{message: message}}} ->
        if message.notice, do: [], else: shown(channel_id) -- [actor]

      {%Channel{}, {:message_created, %V1.MessageCreated{message: message}}} ->
        shown = shown(channel_id)

        named =
          Enum.flat_map(message.mentions, fn
            %V1.Mention{target: {:account_id, id}} -> [Lumia.Proto.from_uuid(id)]
            %V1.Mention{target: {:everyone, true}} -> shown
            _ -> []
          end)

        quoted =
          case message.reply_to do
            %V1.QuoteReply{author_account_id: author} -> [Lumia.Proto.from_uuid(author)]
            nil -> []
          end

        (named ++ quoted)
        |> Enum.filter(&(&1 in shown))
        |> Enum.reject(&(&1 == actor))
        |> Enum.uniq()

      _ ->
        []
    end
  end

  # Who the channel is shown to: in it and able to see it.
  defp shown(channel_id) do
    Repo.all(
      from v in Lumia.Servers.ChannelViewer,
        where: v.channel_id == ^channel_id,
        select: v.account_id
    )
  end

  @doc """
  Sends the notice to every offline device of the recipients, each with
  its account's badge (architecture 6.6); returns how many went.
  """
  def deliver(channel_id, %V1.Event{} = event) do
    recipients(channel_id, event)
    |> for_accounts()
    |> Enum.reject(&online?(&1.device_id))
    |> send_each(fn account_id -> Map.put(@notice, "badge", Lumia.Reads.badge(account_id)) end)
  end

  @doc """
  The account read a conversation further: its devices that are not
  connected are woken, to take away the notifications of what is now read
  and set the badge (architecture 6.6). Only where a wake can be silent:
  the relay's APNs and FCM and UnifiedPush; Web Push shows every push it
  gets, so a browser is left to the account log when it is next open.
  Enqueued after the commit, once for a burst of reads.
  """
  def read_elsewhere(account_id) do
    %{account_id: account_id}
    |> Workers.ReadPush.new()
    |> Oban.insert()

    :ok
  end

  @doc "Carries out `read_elsewhere/1`; returns how many went."
  def deliver_read(account_id) do
    [account_id]
    |> for_accounts()
    |> Enum.filter(&(String.starts_with?(&1.kind, "relay_") or &1.kind == "unified_push"))
    |> Enum.reject(&online?(&1.device_id))
    |> send_each(fn account_id ->
      %{"kind" => "read", "badge" => Lumia.Reads.badge(account_id)}
    end)
  end

  defp send_each(subscriptions, notice_for) do
    notices =
      subscriptions |> Enum.map(& &1.account_id) |> Enum.uniq() |> Map.new(&{&1, notice_for.(&1)})

    Enum.reduce(subscriptions, 0, fn sub, sent ->
      case Lumia.Push.Sender.send(sub, notices[sub.account_id]) do
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
