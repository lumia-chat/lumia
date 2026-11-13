defmodule Lumia.Snapshots do
  @moduledoc """
  Snapshots and listings for the resume paths (docs/specs/delivery.md
  section 8). A snapshot reads the log head first, then state with
  `seq <= head`, so it is exactly consistent at `seq` without a
  serializable transaction.
  """

  import Ecto.Query

  alias Lumia.Accounts
  alias Lumia.Accounts.Account
  alias Lumia.Authz.Access
  alias Lumia.Dms
  alias Lumia.Events
  alias Lumia.Events.Message
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.Servers.Member
  alias Lumia.Servers.MemberRole
  alias Lumia.Servers.Server
  alias Lumia.V1

  @snapshot_messages 50

  @doc "The account snapshot: account, servers, memberships, the channels it is shown, at the account log head."
  def account(account_id) do
    account = Repo.get!(Account, account_id)
    seq = account.last_seq

    memberships = Repo.all(from m in Member, where: m.account_id == ^account_id)
    server_ids = Enum.map(memberships, & &1.server_id)
    servers = Repo.all(from s in Server, where: s.id in ^server_ids)

    role_ids =
      from(mr in MemberRole,
        where: mr.account_id == ^account_id,
        select: {mr.server_id, mr.role_id}
      )
      |> Repo.all()
      |> Enum.group_by(&elem(&1, 0), &elem(&1, 1))

    # Every channel the account is shown: its servers' channels it is in
    # and can view (architecture 6.8), its server DMs among them, then the
    # personal DMs it is in (with their participants and host).
    channels =
      server_ids
      |> Enum.flat_map(&Access.viewable_channels(account_id, &1, joined: true))
      |> Enum.map(& &1.channel)
      |> Enum.concat(Dms.personal_channels(account_id))
      |> Enum.map(&Proto.channel/1)

    %V1.AccountSnapshot{
      account: Proto.account(account, own: true),
      seq: seq,
      servers: Enum.map(servers, &Proto.server/1),
      memberships:
        Enum.map(memberships, fn m ->
          Proto.member(m, Map.get(role_ids, m.server_id, []), server_permissions(m))
        end),
      channels: channels,
      pending_requests: Enum.map(Lumia.Dms.pending_requests_to(account_id), &Proto.dm_request/1),
      channel_invitations: Lumia.Channels.invitations(account_id),
      read_states: Lumia.Reads.states(account_id)
    }
  end

  # What the account may do in a server as a whole, for the client to know
  # what to offer (architecture 6.7).
  defp server_permissions(%Member{server_id: server_id, account_id: account_id}) do
    case Lumia.Servers.actor(server_id, account_id) do
      {:ok, actor} -> actor.server_permissions
      _ -> 0
    end
  end

  @doc """
  The channel snapshot for `viewer`: the channel, newest messages (with
  their reactions as the viewer sees them) and pins at the log head. A
  `PERSONAL_*` channel has no server-side messages; its snapshot carries
  the newest encrypted events instead (protocol section 15).
  """
  def channel(%{channel: %{kind: kind} = channel}, viewer \\ nil) do
    if Dms.personal_kind?(kind),
      do: encrypted_channel(channel),
      else: plain_channel(channel, viewer)
  end

  defp encrypted_channel(channel) do
    head = channel.last_seq
    after_seq = max(head - @snapshot_messages, 0)
    events = Events.events_after(channel.id, after_seq, @snapshot_messages)
    %V1.ChannelSnapshot{channel: Proto.channel(channel), seq: head, encrypted_events: events}
  end

  defp plain_channel(channel, viewer) do
    head = channel.last_seq

    rows =
      from(m in Message,
        where: m.channel_id == ^channel.id and m.seq <= ^head and is_nil(m.parent_id),
        order_by: [desc: m.seq],
        limit: @snapshot_messages
      )
      |> Repo.all()
      |> Enum.reverse()

    messages = with_reactions(rows, viewer)

    %V1.ChannelSnapshot{channel: Proto.channel(channel), seq: head, messages: messages}
  end

  @doc """
  Scrollback for `viewer`: messages with seq strictly below `before_seq`
  (0 = head), newest last, with their reactions as the viewer sees them.
  """
  def messages(channel_id, before_seq, limit, parent_id \\ nil, viewer \\ nil) do
    limit = min(max(limit, 1), 200)

    query =
      from m in Message,
        where: m.channel_id == ^channel_id,
        order_by: [desc: m.seq],
        limit: ^(limit + 1)

    query = if before_seq > 0, do: where(query, [m], m.seq < ^before_seq), else: query

    query =
      if parent_id,
        do: where(query, [m], m.parent_id == ^parent_id),
        else: where(query, [m], is_nil(m.parent_id))

    rows = Repo.all(query)
    has_more = length(rows) > limit
    messages = rows |> Enum.take(limit) |> Enum.reverse() |> with_reactions(viewer)
    %V1.ListMessagesResponse{messages: messages, has_more: has_more}
  end

  defp with_reactions(rows, viewer) do
    reactions = reactions(Enum.map(rows, & &1.id), viewer)
    Enum.map(rows, &Proto.message(&1, Map.get(reactions, &1.id, [])))
  end

  @doc """
  The reactions to each of `message_ids`, as `viewer` sees them: a map from
  message id to `Lumia.V1.ReactionCount`s, in the order each emoji was
  first used, with how many, whether the viewer is among them, and the
  first fifty who reacted.
  """
  def reactions([], _viewer), do: %{}

  def reactions(message_ids, viewer) do
    from(r in Lumia.Events.MessageReaction,
      where: r.message_id in ^message_ids,
      order_by: [r.created_at, r.account_id]
    )
    |> Repo.all()
    |> Enum.group_by(& &1.message_id)
    |> Map.new(fn {message_id, rows} ->
      counts =
        rows
        |> Enum.group_by(& &1.reaction)
        |> Enum.sort_by(fn {_emoji, [first | _]} -> {first.created_at, first.account_id} end)
        |> Enum.map(fn {emoji, who} ->
          %V1.ReactionCount{
            reaction: emoji,
            count: length(who),
            me: Enum.any?(who, &(&1.account_id == viewer)),
            account_ids: who |> Enum.take(50) |> Enum.map(&Proto.uuid(&1.account_id))
          }
        end)

      {message_id, counts}
    end)
  end

  @doc "Account events after a cursor, for `GET /account/events`."
  def account_events(account_id, after_seq, limit) do
    limit = min(max(limit, 1), 200)
    events = Accounts.events_after(account_id, after_seq, limit)

    %V1.ListAccountEventsResponse{
      events: events,
      last_seq: Accounts.head(account_id) || 0,
      truncated_before: false
    }
  end
end
