defmodule Lumia.Channels do
  @moduledoc """
  Channel membership on top of permission (architecture 6.8): joining,
  leaving, adding and removing, the invitations that wait for a member
  who takes adds that way, a server's default channels, Browse, and 1:1
  server DMs (architecture 6.4).

  Permission decides who may see a channel and join it; membership
  decides whose sidebar it is in, who it is delivered to and who may post
  there (`Lumia.Authz.Access`). Every change of membership settles the
  account's `channel_viewers` row in the same transaction, appending
  `ChannelAdded` or `ChannelRemoved` to its log when what it is shown
  changed; the viewer diff (`Lumia.Workers.ViewerDiff`) keeps the same
  rows for changes of permission.
  """

  import Ecto.Query

  alias Lumia.Accounts
  alias Lumia.Audit
  alias Lumia.Authz.Access
  alias Lumia.Authz.Permission
  alias Lumia.Events
  alias Lumia.Events.Channel
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.Servers
  alias Lumia.Servers.ChannelInvitation
  alias Lumia.Servers.ChannelMember
  alias Lumia.Servers.ChannelViewer
  alias Lumia.Servers.Member
  alias Lumia.Tx
  alias Lumia.V1

  # ---- Reading -------------------------------------------------------------------

  @doc "The accounts in a channel, in the order they came in."
  def member_ids(channel_id) do
    Repo.all(
      from m in ChannelMember,
        where: m.channel_id == ^channel_id,
        order_by: [m.joined_at, m.account_id],
        select: m.account_id
    )
  end

  @doc """
  Who is in a channel the account can see, as account ids: those still
  members of its server. `{:error, :not_found}` for a channel it cannot.
  """
  def members(channel_id, account_id) do
    with {:ok, access} <- Access.viewable_channel(account_id, channel_id) do
      {:ok,
       Repo.all(
         from cm in ChannelMember,
           join: m in Member,
           on: m.account_id == cm.account_id and m.server_id == ^access.server_id,
           where: cm.channel_id == ^channel_id,
           order_by: [cm.joined_at, cm.account_id],
           select: cm.account_id
       )}
    end
  end

  @doc """
  Browse (architecture 6.8): the public text channels of a server the
  account can see and is not in, as `{channel, member_count}`, in sidebar
  order. A private channel is never listed.
  """
  def browse(server_id, account_id) do
    channels =
      account_id
      |> Access.viewable_channels(server_id)
      |> Enum.filter(&(&1.channel.kind == "text" and not &1.channel.private and not &1.joined?))
      |> Enum.map(& &1.channel)

    counts = member_counts(server_id, Enum.map(channels, & &1.id))
    Enum.map(channels, &{&1, Map.get(counts, &1.id, 0)})
  end

  defp member_counts(_server_id, []), do: %{}

  defp member_counts(server_id, channel_ids) do
    Repo.all(
      from cm in ChannelMember,
        join: m in Member,
        on: m.account_id == cm.account_id and m.server_id == ^server_id,
        where: cm.channel_id in ^channel_ids,
        group_by: cm.channel_id,
        select: {cm.channel_id, count()}
    )
    |> Map.new()
  end

  @doc """
  The channels the account was asked into and has not answered, as
  `Lumia.V1.ChannelInvitation`s, oldest first: only those of servers it is
  still in.
  """
  def invitations(account_id) do
    Repo.all(
      from i in ChannelInvitation,
        join: c in Channel,
        on: c.id == i.channel_id,
        join: m in Member,
        on: m.server_id == c.server_id and m.account_id == i.account_id,
        where: i.account_id == ^account_id and is_nil(c.deleted_at),
        order_by: i.created_at,
        select: {i, c}
    )
    |> Enum.map(fn {invitation, channel} -> invitation_proto(invitation, channel) end)
  end

  defp invitation_proto(invitation, channel) do
    %V1.ChannelInvitation{
      channel: Proto.channel(channel),
      invited_by_account_id: Proto.uuid(invitation.invited_by_account_id),
      created_at: Proto.timestamp(invitation.created_at)
    }
  end

  # ---- Joining and leaving --------------------------------------------------------

  @doc """
  The account joins a text channel of a server it is in: a public one it
  can see, or one it was invited to. Joining twice is fine. A channel it
  may not join is `{:error, :not_found}`, so a private one is not
  disclosed. Posts "joined". Returns the channel.
  """
  def join(channel_id, account_id) do
    Tx.transact(fn ->
      with {:ok, channel} <- text_channel(channel_id, account_id),
           do: join_channel(channel, account_id)
    end)
  end

  defp join_channel(channel, account_id) do
    cond do
      Access.joined?(channel.id, account_id) -> {:ok, channel}
      may_join?(channel, account_id) -> enter(channel, account_id, nil)
      true -> {:error, :not_found}
    end
  end

  defp may_join?(channel, account_id) do
    invited?(channel.id, account_id) or
      (not channel.private and Access.enterable?(account_id, channel.id))
  end

  @doc """
  The account leaves a text channel, a default one included. Posts "left".
  Leaving a channel one is not in is `{:error, :not_found}`.
  """
  def leave(channel_id, account_id) do
    Tx.transact(fn ->
      with {:ok, channel} <- text_channel(channel_id, account_id),
           :ok <- depart(channel, account_id),
           {:ok, _} <- Events.append_notice(channel.id, account_id, "left", account_id) do
        {:ok, :ok}
      end
    end)
    |> unwrap()
  end

  @doc """
  The newcomer `account_id` joins the server's default channels it can
  see, and `also` (the channel an invite named, or nil) when it may join
  it, each with "joined" (architecture 6.8). Called inside the transaction
  that makes the membership; a channel it cannot enter is passed over.
  """
  def arrive(server_id, account_id, also \\ nil) do
    defaults =
      Repo.all(
        from c in Channel,
          where:
            c.server_id == ^server_id and c.kind == "text" and c.is_default and
              is_nil(c.deleted_at),
          order_by: [c.position, c.id]
      )

    named =
      case also && Repo.get(Channel, also) do
        %Channel{server_id: ^server_id, kind: "text", deleted_at: nil} = channel -> [channel]
        _ -> []
      end

    (defaults ++ named)
    |> Enum.uniq_by(& &1.id)
    |> Enum.filter(&may_join?(&1, account_id))
    |> each(&enter(&1, account_id, nil))
  end

  # ---- Adding and removing ---------------------------------------------------------

  @doc """
  `actor_id`, who is in the channel (or holds `MANAGE_CHANNELS`), adds
  members of the server to it, public or private (architecture 6.8). One
  who takes adds as invitations is invited instead; one already in it, or
  already invited, is left as they are. Every account must be a member of
  the server who could see the channel once in it, or nothing is done and
  the answer is `{:error, {:invalid_argument, "account_ids"}}`. Returns
  `{:ok, %{added: ids, invited: ids}}`.
  """
  def add(channel_id, actor_id, account_ids) when is_list(account_ids) do
    Tx.transact(fn ->
      with {:ok, channel} <- text_channel(channel_id, actor_id),
           :ok <- may_add(channel, actor_id),
           {:ok, targets} <- targets(channel, Enum.uniq(account_ids)) do
        Enum.reduce_while(
          targets,
          {:ok, %{added: [], invited: []}},
          &add_next(channel, actor_id, &1, &2)
        )
      end
    end)
  end

  defp add_next(channel, actor_id, member, {:ok, acc}) do
    case add_one(channel, actor_id, member) do
      {:ok, :added} -> {:cont, {:ok, %{acc | added: acc.added ++ [member.account_id]}}}
      {:ok, :invited} -> {:cont, {:ok, %{acc | invited: acc.invited ++ [member.account_id]}}}
      {:ok, :unchanged} -> {:cont, {:ok, acc}}
      error -> {:halt, error}
    end
  end

  defp may_add(channel, actor_id) do
    cond do
      Access.joined?(channel.id, actor_id) ->
        :ok

      manages_channels?(channel.server_id, actor_id) ->
        :ok

      match?({:ok, _}, Access.viewable_channel(actor_id, channel.id)) ->
        {:error, :permission_denied}

      true ->
        {:error, :not_found}
    end
  end

  defp targets(channel, account_ids) do
    members =
      Repo.all(
        from m in Member,
          where: m.server_id == ^channel.server_id and m.account_id in ^account_ids
      )

    if length(members) == length(account_ids) and
         Enum.all?(members, &Access.enterable?(&1.account_id, channel.id)),
       do: {:ok, members},
       else: {:error, {:invalid_argument, "account_ids"}}
  end

  defp add_one(channel, actor_id, %Member{account_id: account_id} = member) do
    cond do
      Access.joined?(channel.id, account_id) ->
        {:ok, :unchanged}

      account_id == actor_id ->
        with {:ok, _} <- enter(channel, account_id, nil), do: {:ok, :added}

      member.channel_adds_by_invitation ->
        invite(channel, actor_id, account_id)

      true ->
        with {:ok, _} <- enter(channel, account_id, actor_id),
             {:ok, _} <-
               Audit.record({:server, channel.server_id}, actor_id, "channel.add_member",
                 target: {:account, account_id},
                 after: %{"channel_id" => channel.id}
               ) do
          {:ok, :added}
        end
    end
  end

  defp invite(channel, actor_id, account_id) do
    if invited?(channel.id, account_id) do
      {:ok, :unchanged}
    else
      invitation = %ChannelInvitation{
        channel_id: channel.id,
        account_id: account_id,
        invited_by_account_id: actor_id,
        created_at: DateTime.utc_now()
      }

      with {:ok, invitation} <- Repo.insert(invitation),
           {:ok, _} <-
             Accounts.append_event(
               account_id,
               {:channel_invited, invitation_proto(invitation, channel)}
             ),
           {:ok, _} <-
             Audit.record({:server, channel.server_id}, actor_id, "channel.invite",
               target: {:account, account_id},
               after: %{"channel_id" => channel.id}
             ) do
        {:ok, :invited}
      end
    end
  end

  @doc """
  `actor_id`, holding `MANAGE_CHANNELS`, takes `account_id` out of a text
  channel and posts "removed"; taking oneself out is leaving. Someone not
  in the channel is `{:error, :not_found}`.
  """
  def remove(channel_id, actor_id, account_id) when actor_id == account_id,
    do: leave(channel_id, account_id)

  def remove(channel_id, actor_id, account_id) do
    Tx.transact(fn ->
      with {:ok, channel} <- text_channel(channel_id, actor_id),
           true <- manages_channels?(channel.server_id, actor_id) || {:error, :permission_denied},
           :ok <- depart(channel, account_id),
           {:ok, _} <- Events.append_notice(channel.id, actor_id, "removed", account_id),
           {:ok, _} <-
             Audit.record({:server, channel.server_id}, actor_id, "channel.remove_member",
               target: {:account, account_id},
               after: %{"channel_id" => channel.id}
             ) do
        {:ok, :ok}
      end
    end)
    |> unwrap()
  end

  @doc "The account says no to an invitation. One it does not have is `{:error, :not_found}`."
  def dismiss(channel_id, account_id) do
    Tx.transact(fn -> dismiss_invitation(channel_id, account_id) end)
    |> unwrap()
  end

  defp dismiss_invitation(channel_id, account_id) do
    if invited?(channel_id, account_id) do
      with :ok <- drop_invitation(channel_id, account_id), do: {:ok, :ok}
    else
      {:error, :not_found}
    end
  end

  # ---- Default channels -----------------------------------------------------------

  @doc """
  Makes exactly `channel_ids` the server's default channels (architecture
  6.8): public text channels of the server, or
  `{:error, {:invalid_argument, "default_channels"}}`. `MANAGE_SERVER`.
  Nobody already in the server is moved by it.
  """
  def set_defaults(server_id, actor_id, channel_ids) when is_list(channel_ids) do
    ids = Enum.uniq(channel_ids)

    Tx.transact(fn ->
      with {:ok, actor} <- Servers.actor(server_id, actor_id),
           true <-
             (actor.owner? or Permission.has?(actor.server_permissions, :manage_server)) ||
               {:error, :permission_denied},
           :ok <- check_defaults(server_id, ids),
           do: make_defaults(server_id, actor_id, ids)
    end)
  end

  defp make_defaults(server_id, actor_id, ids) do
    before =
      Repo.all(from c in Channel, where: c.server_id == ^server_id and c.is_default, select: c.id)

    Repo.update_all(
      from(c in Channel, where: c.server_id == ^server_id and c.kind == "text"),
      set: [is_default: false]
    )

    Repo.update_all(from(c in Channel, where: c.id in ^ids), set: [is_default: true])

    with {:ok, _} <-
           Audit.record({:server, server_id}, actor_id, "server.default_channels",
             before: %{"channel_ids" => before},
             after: %{"channel_ids" => ids}
           ) do
      {:ok, ids}
    end
  end

  defp check_defaults(server_id, ids) do
    found =
      Repo.aggregate(
        from(c in Channel,
          where:
            c.id in ^ids and c.server_id == ^server_id and c.kind == "text" and not c.private and
              is_nil(c.deleted_at)
        ),
        :count
      )

    if found == length(ids), do: :ok, else: {:error, {:invalid_argument, "default_channels"}}
  end

  # ---- Server DMs -----------------------------------------------------------------

  @doc """
  The 1:1 server DM between `actor_id` and `account_id` in a server
  (architecture 6.4), started if there is none: `{:ok, channel}`. The
  actor needs `START_SERVER_DMS`; starting one with someone who turned
  server DMs off there is `{:error, {:failed_precondition,
  "server_dms_off"}}`, and with someone either has blocked is
  `{:error, :not_found}`. One already started is returned as it is.
  """
  def open_dm(server_id, actor_id, account_id) do
    Tx.transact(fn ->
      with {:ok, actor} <- Servers.actor(server_id, actor_id),
           true <- actor_id != account_id || {:error, :invalid_argument},
           %Member{} = other <-
             Repo.get_by(Member, server_id: server_id, account_id: account_id) ||
               {:error, :not_found},
           false <- Lumia.Dms.blocked_between?(actor_id, account_id) && {:error, :not_found},
           do: existing_or_new_dm(server_id, actor, other)
    end)
  end

  defp existing_or_new_dm(server_id, actor, other) do
    key = dm_key(server_id, actor.account_id, other.account_id)

    case Repo.get_by(Channel, dm_key: key) do
      %Channel{} = channel -> ensure_dm(channel, [actor.account_id, other.account_id])
      nil -> start_dm(server_id, key, actor, other)
    end
  end

  defp start_dm(server_id, key, actor, other) do
    cond do
      not (actor.owner? or Permission.has?(actor.server_permissions, :start_server_dms)) ->
        {:error, :permission_denied}

      other.server_dms_off ->
        {:error, {:failed_precondition, "server_dms_off"}}

      true ->
        %Channel{kind: "server_dm", server_id: server_id, dm_key: key}
        |> Ecto.Changeset.change()
        |> Ecto.Changeset.unique_constraint(:dm_key, name: :channels_dm_key_index)
        |> Repo.insert()
        |> case do
          {:ok, channel} -> ensure_dm(channel, [actor.account_id, other.account_id])
          error -> error
        end
    end
  end

  # Both are in it: one who left the server and came back is again.
  defp ensure_dm(channel, account_ids) do
    now = DateTime.utc_now()

    Enum.each(account_ids, fn id ->
      Repo.insert(%ChannelMember{channel_id: channel.id, account_id: id, joined_at: now},
        on_conflict: :nothing
      )
    end)

    with :ok <- each(account_ids, &settle(channel.id, &1)), do: {:ok, channel}
  end

  defp dm_key(server_id, a, b) do
    [first, second] = Enum.sort([a, b])
    "server:#{server_id}:#{first}:#{second}"
  end

  # ---- Internals --------------------------------------------------------------------

  # A live text channel of a server the account is in.
  defp text_channel(channel_id, account_id) do
    with %Channel{kind: "text", server_id: server_id, deleted_at: nil} = channel
         when is_binary(server_id) <- channel_id && Repo.get(Channel, channel_id),
         %{} <- Servers.member_for(server_id, account_id) do
      {:ok, channel}
    else
      _ -> {:error, :not_found}
    end
  end

  # In, with the notice that says so: "joined" for one who came in by
  # themselves, "added" for one someone else brought.
  defp enter(channel, account_id, added_by) do
    member = %ChannelMember{
      channel_id: channel.id,
      account_id: account_id,
      added_by_account_id: added_by,
      joined_at: DateTime.utc_now()
    }

    {author, notice} = if added_by, do: {added_by, "added"}, else: {account_id, "joined"}

    with {:ok, _} <- Repo.insert(member, on_conflict: :nothing),
         :ok <- drop_invitation(channel.id, account_id),
         :ok <- settle(channel.id, account_id),
         {:ok, _} <- Events.append_notice(channel.id, author, notice, account_id) do
      {:ok, channel}
    end
  end

  @doc false
  # The creator of a channel is in it from the start, with no notice.
  def enter_quietly(channel, account_id) do
    with {:ok, _} <-
           Repo.insert(
             %ChannelMember{
               channel_id: channel.id,
               account_id: account_id,
               joined_at: DateTime.utc_now()
             },
             on_conflict: :nothing
           ),
         :ok <- settle(channel.id, account_id) do
      {:ok, channel}
    end
  end

  defp depart(channel, account_id) do
    case Repo.delete_all(
           from m in ChannelMember,
             where: m.channel_id == ^channel.id and m.account_id == ^account_id
         ) do
      {1, _} -> settle(channel.id, account_id)
      {0, _} -> {:error, :not_found}
    end
  end

  @doc """
  Brings the account's `channel_viewers` row for the channel into line
  with what it is now shown (in it, and able to see it), appending
  `ChannelAdded` or `ChannelRemoved` to its log when that changed.
  """
  def settle(channel_id, account_id) do
    {shown?, channel} =
      case Access.channel(account_id, channel_id) do
        {:ok, %{joined?: joined?, permissions: bits, channel: channel}} ->
          {joined? and Permission.has?(bits, :view_channel), channel}

        _ ->
          {false, nil}
      end

    row =
      from v in ChannelViewer, where: v.channel_id == ^channel_id and v.account_id == ^account_id

    case {shown?, Repo.exists?(row)} do
      {true, false} ->
        with {:ok, _} <-
               Repo.insert(%ChannelViewer{channel_id: channel_id, account_id: account_id}),
             {:ok, _} <-
               Accounts.append_event(
                 account_id,
                 {:channel_added, %V1.ChannelAdded{channel: Proto.channel(channel)}}
               ),
             do: :ok

      {false, true} ->
        Repo.delete_all(row)

        with {:ok, _} <-
               Accounts.append_event(
                 account_id,
                 {:channel_removed, %V1.ChannelRemoved{channel_id: Proto.uuid(channel_id)}}
               ),
             do: :ok

      _ ->
        :ok
    end
  end

  defp invited?(channel_id, account_id) do
    Repo.exists?(
      from i in ChannelInvitation,
        where: i.channel_id == ^channel_id and i.account_id == ^account_id
    )
  end

  defp drop_invitation(channel_id, account_id) do
    case Repo.delete_all(
           from i in ChannelInvitation,
             where: i.channel_id == ^channel_id and i.account_id == ^account_id
         ) do
      {0, _} ->
        :ok

      {_, _} ->
        removed = %V1.ChannelInvitationRemoved{channel_id: Proto.uuid(channel_id)}

        with {:ok, _} <- Accounts.append_event(account_id, {:channel_invitation_removed, removed}),
             do: :ok
    end
  end

  defp manages_channels?(server_id, account_id) do
    case Servers.actor(server_id, account_id) do
      {:ok, actor} -> actor.owner? or Permission.has?(actor.server_permissions, :manage_channels)
      _ -> false
    end
  end

  defp each(items, fun) do
    Enum.reduce_while(items, :ok, fn item, :ok ->
      case fun.(item) do
        :ok -> {:cont, :ok}
        {:ok, _} -> {:cont, :ok}
        error -> {:halt, error}
      end
    end)
  end

  # Tx.transact/1 requires {:ok, _} | {:error, _}; callers want :ok.
  defp unwrap({:ok, :ok}), do: :ok
  defp unwrap(other), do: other
end
