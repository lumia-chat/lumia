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

  @doc "The account snapshot: account, servers, memberships, viewable channels, at the account log head."
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

    # Every channel the account can view: its servers' channels, then the
    # personal DMs it is in (with their participants and host).
    channels =
      server_ids
      |> Enum.flat_map(&Access.viewable_channels(account_id, &1))
      |> Enum.map(& &1.channel)
      |> Enum.concat(Dms.personal_channels(account_id))
      |> Enum.map(&Proto.channel/1)

    %V1.AccountSnapshot{
      account: Proto.account(account, own: true),
      seq: seq,
      servers: Enum.map(servers, &Proto.server/1),
      memberships: Enum.map(memberships, &Proto.member(&1, Map.get(role_ids, &1.server_id, []))),
      channels: channels,
      pending_requests: Enum.map(Lumia.Dms.pending_requests_to(account_id), &Proto.dm_request/1)
    }
  end

  @doc """
  The channel snapshot: the channel, newest messages and pins at the log
  head. A `PERSONAL_*` channel has no server-side messages; its snapshot
  carries the newest encrypted events instead (protocol section 15).
  """
  def channel(%{channel: %{kind: kind} = channel}) do
    if Dms.personal_kind?(kind), do: encrypted_channel(channel), else: plain_channel(channel)
  end

  defp encrypted_channel(channel) do
    head = channel.last_seq
    after_seq = max(head - @snapshot_messages, 0)
    events = Events.events_after(channel.id, after_seq, @snapshot_messages)
    %V1.ChannelSnapshot{channel: Proto.channel(channel), seq: head, encrypted_events: events}
  end

  defp plain_channel(channel) do
    head = channel.last_seq

    messages =
      from(m in Message,
        where: m.channel_id == ^channel.id and m.seq <= ^head and is_nil(m.parent_id),
        order_by: [desc: m.seq],
        limit: @snapshot_messages
      )
      |> Repo.all()
      |> Enum.reverse()
      |> Enum.map(&Proto.message/1)

    %V1.ChannelSnapshot{channel: Proto.channel(channel), seq: head, messages: messages}
  end

  @doc "Scrollback: messages with seq strictly below `before_seq` (0 = head), newest last."
  def messages(channel_id, before_seq, limit, parent_id \\ nil) do
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
    messages = rows |> Enum.take(limit) |> Enum.reverse() |> Enum.map(&Proto.message/1)
    %V1.ListMessagesResponse{messages: messages, has_more: has_more}
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
