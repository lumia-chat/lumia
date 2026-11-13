defmodule Lumia.ChannelsTest do
  @moduledoc """
  Channel membership on top of permission (architecture 6.8) and 1:1
  server DMs (architecture 6.4).
  """

  use Lumia.DataCase, async: true

  import Lumia.Fixtures

  alias Lumia.Accounts
  alias Lumia.Authz.Access
  alias Lumia.Authz.Permission
  alias Lumia.Channels
  alias Lumia.Events
  alias Lumia.Events.Message
  alias Lumia.Servers
  alias Lumia.Snapshots
  alias Lumia.V1

  setup do
    owner = account("owner")
    {:ok, server} = Servers.create_server(owner.id, "Acme")
    {:ok, general} = Servers.create_channel(server.id, owner.id, "general")
    {:ok, design} = Servers.create_channel(server.id, owner.id, "design", topic: "Pixels")
    {:ok, secret} = Servers.create_channel(server.id, owner.id, "secret", private: true)
    grace = account("grace")
    {:ok, _} = Servers.add_member(server.id, grace.id, owner.id)

    %{
      owner: owner,
      server: server,
      general: general,
      design: design,
      secret: secret,
      grace: grace
    }
  end

  # The notices in a channel, oldest first, as {kind, author, about}.
  defp notices(channel_id) do
    Repo.all(
      from m in Message,
        where: m.channel_id == ^channel_id and not is_nil(m.notice),
        order_by: m.seq,
        select: {m.notice, m.author_account_id, m.notice_account_id}
    )
  end

  defp kinds(account), do: Accounts.event_kinds(account.id)

  defp shown(account) do
    for c <- Snapshots.account(account.id).channels, do: Lumia.Proto.from_uuid(c.channel_id)
  end

  defp can?(account, channel, permission) do
    match?({:ok, _}, Access.require(account.id, channel.id, permission))
  end

  describe "arriving" do
    test "the first channel is the default, and a newcomer joins it with a notice", ctx do
      assert ctx.general.is_default
      refute ctx.design.is_default

      assert shown(ctx.grace) == [ctx.general.id]
      assert notices(ctx.general.id) == [{"joined", ctx.grace.id, ctx.grace.id}]
      assert "channel_added" in kinds(ctx.grace)
    end

    test "the creator of a channel is in it from the start, with no notice", ctx do
      assert Enum.sort(shown(ctx.owner)) ==
               Enum.sort([ctx.general.id, ctx.design.id, ctx.secret.id])

      assert notices(ctx.design.id) == []
    end

    test "an invite that names a channel brings its newcomer into it too", ctx do
      ada = account("ada")

      {:ok, invite} =
        Lumia.Invites.create(ctx.server.id, ctx.owner.id, %{channel_id: ctx.design.id})

      {:ok, _} = Lumia.Invites.accept(invite.code, ada.id)
      assert Enum.sort(shown(ada)) == Enum.sort([ctx.general.id, ctx.design.id])
    end
  end

  describe "joining and leaving" do
    test "a public channel is joined, posted in, and left", ctx do
      assert can?(ctx.grace, ctx.design, :read_history)
      refute can?(ctx.grace, ctx.design, :send_messages)

      assert {:ok, _} = Channels.join(ctx.design.id, ctx.grace.id)
      assert ctx.design.id in shown(ctx.grace)
      assert can?(ctx.grace, ctx.design, :send_messages)

      # Joining again changes nothing.
      assert {:ok, _} = Channels.join(ctx.design.id, ctx.grace.id)
      assert notices(ctx.design.id) == [{"joined", ctx.grace.id, ctx.grace.id}]

      assert :ok = Channels.leave(ctx.design.id, ctx.grace.id)
      refute ctx.design.id in shown(ctx.grace)
      refute can?(ctx.grace, ctx.design, :send_messages)
      assert List.last(notices(ctx.design.id)) == {"left", ctx.grace.id, ctx.grace.id}
      assert "channel_removed" in kinds(ctx.grace)
      assert {:error, :not_found} = Channels.leave(ctx.design.id, ctx.grace.id)
    end

    test "a default channel can be left too", ctx do
      assert :ok = Channels.leave(ctx.general.id, ctx.grace.id)
      assert shown(ctx.grace) == []
    end

    test "someone not in a channel cannot write there over the command path", ctx do
      assert {:error, :permission_denied} =
               Access.require(ctx.grace.id, ctx.design.id, :add_reactions)

      # The owner, with everything, is held to it as well.
      :ok = Channels.leave(ctx.design.id, ctx.owner.id)
      refute can?(ctx.owner, ctx.design, :send_messages)
      assert can?(ctx.owner, ctx.design, :manage_messages)
    end

    test "a private channel is not joined, nor disclosed, by someone not added", ctx do
      refute can?(ctx.grace, ctx.secret, :read_history)
      assert {:error, :not_found} = Channels.join(ctx.secret.id, ctx.grace.id)
      assert {:error, :not_found} = Channels.members(ctx.secret.id, ctx.grace.id)
    end

    test "leaving a private channel means being added again to come back", ctx do
      {:ok, %{added: [_]}} = Channels.add(ctx.secret.id, ctx.owner.id, [ctx.grace.id])
      :ok = Channels.leave(ctx.secret.id, ctx.grace.id)
      assert {:error, :not_found} = Channels.join(ctx.secret.id, ctx.grace.id)
    end
  end

  describe "adding and removing" do
    test "anyone in a channel adds a member, who is shown it and told who did", ctx do
      ada = account("ada")
      {:ok, _} = Servers.add_member(ctx.server.id, ada.id, ctx.owner.id)
      {:ok, _} = Channels.join(ctx.design.id, ctx.grace.id)

      assert {:ok, %{added: [added], invited: []}} =
               Channels.add(ctx.design.id, ctx.grace.id, [ada.id])

      assert added == ada.id
      assert ctx.design.id in shown(ada)
      assert List.last(notices(ctx.design.id)) == {"added", ctx.grace.id, ada.id}

      # Adding someone already in it is no change.
      assert {:ok, %{added: [], invited: []}} =
               Channels.add(ctx.design.id, ctx.grace.id, [ada.id])
    end

    test "a member in a private channel adds another, with no overwrite written", ctx do
      ada = account("ada")
      {:ok, _} = Servers.add_member(ctx.server.id, ada.id, ctx.owner.id)
      {:ok, %{added: [_]}} = Channels.add(ctx.secret.id, ctx.owner.id, [ctx.grace.id])
      {:ok, %{added: [_]}} = Channels.add(ctx.secret.id, ctx.grace.id, [ada.id])

      assert can?(ada, ctx.secret, :send_messages)
      assert ctx.secret.id in shown(ada)
      # Only the @everyone deny: a private channel's size is not bound by the overwrite limit.
      assert Repo.aggregate(
               from(o in Lumia.Servers.ChannelOverwrite, where: o.channel_id == ^ctx.secret.id),
               :count
             ) == 1
    end

    test "someone not in a channel, and without MANAGE_CHANNELS, cannot add to it", ctx do
      ada = account("ada")
      {:ok, _} = Servers.add_member(ctx.server.id, ada.id, ctx.owner.id)

      assert {:error, :permission_denied} = Channels.add(ctx.design.id, ctx.grace.id, [ada.id])
      assert {:error, :not_found} = Channels.add(ctx.secret.id, ctx.grace.id, [ada.id])
    end

    test "only members of the server who could see the channel are added", ctx do
      outsider = account("outsider")

      assert {:error, {:invalid_argument, "account_ids"}} =
               Channels.add(ctx.design.id, ctx.owner.id, [outsider.id])

      guest = account("guest")
      {:ok, _} = Servers.add_member(ctx.server.id, guest.id, ctx.owner.id, kind: "guest")

      # A guest sees what overwrites grant it, and membership of a private channel grants nothing.
      assert {:error, {:invalid_argument, "account_ids"}} =
               Channels.add(ctx.secret.id, ctx.owner.id, [guest.id])
    end

    test "one who takes adds as invitations is invited, and joins or dismisses", ctx do
      {:ok, _} =
        Servers.update_membership(ctx.server.id, ctx.grace.id, %{channel_adds_by_invitation: true})

      assert {:ok, %{added: [], invited: [invited]}} =
               Channels.add(ctx.secret.id, ctx.owner.id, [ctx.grace.id])

      assert invited == ctx.grace.id
      refute ctx.secret.id in shown(ctx.grace)
      assert "channel_invited" in kinds(ctx.grace)

      assert [%V1.ChannelInvitation{channel: %V1.Channel{name: "secret", private: true}}] =
               Snapshots.account(ctx.grace.id).channel_invitations

      # Invited twice is one invitation.
      assert {:ok, %{invited: []}} = Channels.add(ctx.secret.id, ctx.owner.id, [ctx.grace.id])

      # The invitation is the way into a private channel.
      assert {:ok, _} = Channels.join(ctx.secret.id, ctx.grace.id)
      assert ctx.secret.id in shown(ctx.grace)
      assert Snapshots.account(ctx.grace.id).channel_invitations == []
      assert "channel_invitation_removed" in kinds(ctx.grace)
      assert List.last(notices(ctx.secret.id)) == {"joined", ctx.grace.id, ctx.grace.id}

      # Dismissed, an invitation is gone; one not held is not found.
      {:ok, %{invited: [_]}} = Channels.add(ctx.design.id, ctx.owner.id, [ctx.grace.id])
      assert :ok = Channels.dismiss(ctx.design.id, ctx.grace.id)
      assert {:error, :not_found} = Channels.dismiss(ctx.design.id, ctx.grace.id)
      refute ctx.design.id in shown(ctx.grace)
    end

    test "removing needs MANAGE_CHANNELS, and says who did it", ctx do
      ada = account("ada")
      {:ok, _} = Servers.add_member(ctx.server.id, ada.id, ctx.owner.id)

      assert {:error, :permission_denied} =
               Channels.remove(ctx.general.id, ctx.grace.id, ada.id)

      assert :ok = Channels.remove(ctx.general.id, ctx.owner.id, ada.id)
      refute ctx.general.id in shown(ada)
      assert List.last(notices(ctx.general.id)) == {"removed", ctx.owner.id, ada.id}

      # Removing oneself is leaving.
      assert :ok = Channels.remove(ctx.general.id, ctx.grace.id, ctx.grace.id)
      assert List.last(notices(ctx.general.id)) == {"left", ctx.grace.id, ctx.grace.id}
    end

    test "leaving the server leaves its channels and invitations", ctx do
      {:ok, _} = Channels.join(ctx.design.id, ctx.grace.id)

      {:ok, _} =
        Servers.update_membership(ctx.server.id, ctx.grace.id, %{channel_adds_by_invitation: true})

      {:ok, %{invited: [_]}} = Channels.add(ctx.secret.id, ctx.owner.id, [ctx.grace.id])
      :ok = Servers.remove_member(ctx.server.id, ctx.grace.id, ctx.grace.id, "left")

      {:ok, _} = Servers.add_member(ctx.server.id, ctx.grace.id, ctx.owner.id)
      assert shown(ctx.grace) == [ctx.general.id]
      assert Snapshots.account(ctx.grace.id).channel_invitations == []
    end

    test "the channel's members are listed to whoever can see it", ctx do
      assert {:ok, ids} = Channels.members(ctx.general.id, ctx.grace.id)
      assert ids == [ctx.owner.id, ctx.grace.id]
    end
  end

  describe "browse" do
    test "lists the public channels one can see and is not in, with how many are", ctx do
      assert [{design, 1}] = Channels.browse(ctx.server.id, ctx.grace.id)
      assert design.id == ctx.design.id

      {:ok, _} = Channels.join(ctx.design.id, ctx.grace.id)
      assert Channels.browse(ctx.server.id, ctx.grace.id) == []
    end

    test "a private channel is never in it, even for one who can see it", ctx do
      :ok = Channels.leave(ctx.secret.id, ctx.owner.id)
      refute Enum.any?(Channels.browse(ctx.server.id, ctx.owner.id), &elem(&1, 0).private)
    end
  end

  describe "default channels" do
    test "MANAGE_SERVER chooses them among the public channels; newcomers join them", ctx do
      assert {:error, :permission_denied} =
               Channels.set_defaults(ctx.server.id, ctx.grace.id, [ctx.design.id])

      assert {:error, {:invalid_argument, "default_channels"}} =
               Channels.set_defaults(ctx.server.id, ctx.owner.id, [ctx.secret.id])

      assert {:ok, _} =
               Channels.set_defaults(ctx.server.id, ctx.owner.id, [ctx.general.id, ctx.design.id])

      ada = account("ada")
      {:ok, _} = Servers.add_member(ctx.server.id, ada.id, ctx.owner.id)
      assert Enum.sort(shown(ada)) == Enum.sort([ctx.general.id, ctx.design.id])

      # Nobody already in the server is moved.
      assert shown(ctx.grace) == [ctx.general.id]

      assert {:ok, []} = Channels.set_defaults(ctx.server.id, ctx.owner.id, [])
      bo = account("bo")
      {:ok, _} = Servers.add_member(ctx.server.id, bo.id, ctx.owner.id)
      assert shown(bo) == []
    end
  end

  describe "server DMs" do
    test "one is started once, both are shown it, and its messages are pushed", ctx do
      assert {:ok, dm} = Channels.open_dm(ctx.server.id, ctx.grace.id, ctx.owner.id)
      assert dm.kind == "server_dm"
      assert {:ok, same} = Channels.open_dm(ctx.server.id, ctx.owner.id, ctx.grace.id)
      assert same.id == dm.id

      assert dm.id in shown(ctx.grace)
      assert dm.id in shown(ctx.owner)

      assert %V1.Channel{kind: :SERVER_DM, participant_account_ids: participants} =
               Lumia.Proto.channel(dm)

      assert Enum.sort(participants) ==
               Enum.sort([Lumia.Proto.uuid(ctx.grace.id), Lumia.Proto.uuid(ctx.owner.id)])

      {:ok, _} = Events.append(dm.id, actor(ctx.grace), send_message(dm.id, "hi"))
      [event] = Events.events_after(dm.id, 0)
      assert Lumia.Push.recipients(dm.id, event) == [ctx.owner.id]
    end

    test "is its members' alone: an administrator of the server cannot read it", ctx do
      ada = account("ada")
      {:ok, _} = Servers.add_member(ctx.server.id, ada.id, ctx.owner.id)
      {:ok, dm} = Channels.open_dm(ctx.server.id, ctx.grace.id, ada.id)

      assert {:error, :not_found} = Access.require(ctx.owner.id, dm.id, :read_history)
      assert can?(ada, dm, :send_messages)
    end

    test "needs START_SERVER_DMS, and someone who has not turned them off", ctx do
      ada = account("ada")
      {:ok, _} = Servers.add_member(ctx.server.id, ada.id, ctx.owner.id)
      {:ok, _} = Servers.update_membership(ctx.server.id, ada.id, %{server_dms_off: true})

      assert {:error, {:failed_precondition, "server_dms_off"}} =
               Channels.open_dm(ctx.server.id, ctx.grace.id, ada.id)

      everyone = Repo.get_by!(Servers.Role, server_id: ctx.server.id, is_everyone: true)

      {:ok, _} =
        Servers.update_role(ctx.server.id, everyone.id, ctx.owner.id, %{
          permissions: Permission.from_bits(everyone.permissions) -- [:start_server_dms]
        })

      assert {:error, :permission_denied} =
               Channels.open_dm(ctx.server.id, ctx.grace.id, ctx.owner.id)

      assert {:error, :invalid_argument} =
               Channels.open_dm(ctx.server.id, ctx.owner.id, ctx.owner.id)

      outsider = account("outsider")

      assert {:error, :not_found} = Channels.open_dm(ctx.server.id, ctx.owner.id, outsider.id)
    end

    test "a block either way stops a new one and anything more said in one", ctx do
      ada = account("ada")
      {:ok, _} = Servers.add_member(ctx.server.id, ada.id, ctx.owner.id)
      {:ok, dm} = Channels.open_dm(ctx.server.id, ctx.grace.id, ada.id)

      {:ok, request} = Lumia.Dms.send_request(ctx.grace.id, ada.id)
      {:ok, _, _} = Lumia.Dms.respond(request.id, ada.id, :blocked)

      assert can?(ctx.grace, dm, :read_history)
      refute can?(ctx.grace, dm, :send_messages)
      refute can?(ada, dm, :send_messages)

      bo = account("bo")
      {:ok, _} = Servers.add_member(ctx.server.id, bo.id, ctx.owner.id)
      {:ok, request} = Lumia.Dms.send_request(bo.id, ada.id)
      {:ok, _, _} = Lumia.Dms.respond(request.id, ada.id, :blocked)
      assert {:error, :not_found} = Channels.open_dm(ctx.server.id, ada.id, bo.id)
    end

    test "is hidden from one who left the server, and theirs again when they come back", ctx do
      {:ok, dm} = Channels.open_dm(ctx.server.id, ctx.grace.id, ctx.owner.id)
      :ok = Servers.remove_member(ctx.server.id, ctx.grace.id, ctx.grace.id, "left")
      refute dm.id in shown(ctx.grace)

      {:ok, _} = Servers.add_member(ctx.server.id, ctx.grace.id, ctx.owner.id)
      assert {:ok, again} = Channels.open_dm(ctx.server.id, ctx.owner.id, ctx.grace.id)
      assert again.id == dm.id
      assert dm.id in shown(ctx.grace)
    end
  end

  describe "notices and mentions" do
    test "a notice cannot be edited or deleted", ctx do
      [{_, _, _}] = notices(ctx.general.id)
      notice = Repo.one!(from m in Message, where: m.channel_id == ^ctx.general.id)

      assert {:error, :message_not_found} =
               Events.append(
                 ctx.general.id,
                 actor(ctx.grace),
                 edit_message(ctx.general.id, notice.id, "rewritten")
               )
    end

    test "a mention notifies only who is in the channel", ctx do
      ada = account("ada")
      {:ok, _} = Servers.add_member(ctx.server.id, ada.id, ctx.owner.id)
      {:ok, _} = Channels.join(ctx.design.id, ada.id)

      mention = fn account ->
        %V1.Mention{target: {:account_id, Lumia.Proto.uuid(account.id)}}
      end

      command = send_message(ctx.design.id, "hey")
      {:send_message, op} = command.op

      command = %{
        command
        | op: {:send_message, %{op | mentions: [mention.(ada), mention.(ctx.grace)]}}
      }

      {:ok, %{seq: seq}} = Events.append(ctx.design.id, actor(ctx.owner), command)
      [event] = Events.events_after(ctx.design.id, seq - 1, 1)
      assert Lumia.Push.recipients(ctx.design.id, event) == [ada.id]
    end
  end

  describe "switches" do
    test "a member sets their own, and their other devices are told with their permissions",
         ctx do
      {:ok, member} =
        Servers.update_membership(ctx.server.id, ctx.grace.id, %{
          channel_adds_by_invitation: true,
          server_dms_off: true
        })

      assert member.channel_adds_by_invitation and member.server_dms_off

      [%V1.AccountEvent{payload: {:membership_updated, %V1.MembershipUpdated{member: told}}}] =
        for %V1.AccountEvent{payload: {:membership_updated, _}} = e <-
              Accounts.events_after(ctx.grace.id, 0),
            do: e

      assert told.channel_adds_by_invitation and told.server_dms_off
      assert :VIEW_CHANNEL in told.permissions
    end
  end
end
