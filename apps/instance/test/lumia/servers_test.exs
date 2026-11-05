defmodule Lumia.ServersTest do
  use Lumia.DataCase, async: true
  use Oban.Testing, repo: Lumia.Repo

  alias Lumia.Accounts
  alias Lumia.Audit.Entry
  alias Lumia.Authz.Cache
  alias Lumia.Authz.Permission
  alias Lumia.Events.Channel
  alias Lumia.Servers
  alias Lumia.Servers.Role
  alias Lumia.Workers.AclChanged
  alias Lumia.Workers.ViewerDiff

  defp account(name) do
    {:ok, a} = Accounts.create_account(%{handle: "#{name}#{System.unique_integer([:positive])}"})
    a
  end

  setup do
    owner = account("owner")
    {:ok, server} = Servers.create_server(owner.id, "Acme")
    {:ok, general} = Servers.create_channel(server.id, owner.id, "general")
    {:ok, random} = Servers.create_channel(server.id, owner.id, "random")
    {:ok, other} = Servers.create_server(account("other").id, "Other")
    {:ok, other_channel} = Servers.create_channel(other.id, other.owner_account_id, "lobby")

    %{
      owner: owner,
      server: server,
      general: general,
      random: random,
      other_channel: other_channel
    }
  end

  defp acl_versions(ids),
    do:
      Repo.all(from c in Channel, where: c.id in ^ids, select: {c.id, c.acl_version}) |> Map.new()

  defp account_kinds(account_id), do: Accounts.event_kinds(account_id)

  describe "create_server/2" do
    test "creates @everyone, the owner's membership, and an audit row", ctx do
      assert %Role{is_everyone: true} =
               Repo.get_by!(Role, server_id: ctx.server.id, is_everyone: true)

      assert Servers.member_for(ctx.server.id, ctx.owner.id).owner?

      assert Repo.exists?(
               from e in Entry,
                 where: e.scope_id == ^ctx.server.id and e.action == "server.create"
             )
    end
  end

  describe "definition-level changes" do
    @tag failure_injection: [authorization: 1]
    test "a role change bumps every channel in the server and nothing elsewhere", ctx do
      before = acl_versions([ctx.general.id, ctx.random.id, ctx.other_channel.id])

      {:ok, role} =
        Servers.create_role(ctx.server.id, ctx.owner.id, %{
          name: "mods",
          permissions: [:manage_messages]
        })

      {:ok, _} =
        Servers.update_role(ctx.server.id, role.id, ctx.owner.id, %{
          permissions: [:manage_messages, :attach_files]
        })

      after_ = acl_versions([ctx.general.id, ctx.random.id, ctx.other_channel.id])
      assert after_[ctx.general.id] == before[ctx.general.id] + 2
      assert after_[ctx.random.id] == before[ctx.random.id] + 2
      assert after_[ctx.other_channel.id] == before[ctx.other_channel.id]

      assert_enqueued(worker: AclChanged, args: %{server_id: ctx.server.id})
      assert :ok = perform_job(AclChanged, %{"server_id" => ctx.server.id})

      assert [%Lumia.V1.Event{payload: {:acl_changed, _}}] =
               Lumia.Events.events_after(ctx.general.id, 0)
    end

    test "an overwrite bumps that channel only and appends acl_changed", ctx do
      before = acl_versions([ctx.general.id, ctx.random.id])
      everyone = Repo.get_by!(Role, server_id: ctx.server.id, is_everyone: true)

      :ok =
        Servers.set_overwrite(ctx.general.id, ctx.owner.id, {:role, everyone.id}, [], [
          :view_channel
        ])

      after_ = acl_versions([ctx.general.id, ctx.random.id])
      assert after_[ctx.general.id] == before[ctx.general.id] + 1
      assert after_[ctx.random.id] == before[ctx.random.id]

      assert [%Lumia.V1.Event{payload: {:acl_changed, _}}] =
               Lumia.Events.events_after(ctx.general.id, 0)
    end
  end

  describe "overwrite targets and role limits" do
    test "an overwrite for a role or member of another server is NOT_FOUND", ctx do
      stranger = account("stranger")

      other_everyone =
        Repo.get_by!(Role, server_id: ctx.other_channel.server_id, is_everyone: true)

      assert {:error, :not_found} =
               Servers.set_overwrite(
                 ctx.general.id,
                 ctx.owner.id,
                 {:role, other_everyone.id},
                 [],
                 [
                   :view_channel
                 ]
               )

      assert {:error, :not_found} =
               Servers.set_overwrite(ctx.general.id, ctx.owner.id, {:member, stranger.id}, [], [
                 :view_channel
               ])

      assert {:error, :not_found} =
               Servers.set_overwrite(ctx.general.id, ctx.owner.id, {:member, nil}, [], [])

      assert Repo.aggregate(Lumia.Servers.ChannelOverwrite, :count) == 0
    end

    test "an overwrite for a member of this server is accepted", ctx do
      alice = account("alice")
      {:ok, _} = Servers.add_member(ctx.server.id, alice.id, ctx.owner.id)

      assert :ok =
               Servers.set_overwrite(ctx.general.id, ctx.owner.id, {:member, alice.id}, [], [
                 :send_messages
               ])
    end

    test "roles_per_member is enforced by assign_role and set_roles", ctx do
      Application.put_env(:lumia, :limits, roles_per_member: 2)
      on_exit(fn -> Application.delete_env(:lumia, :limits) end)

      alice = account("alice")
      {:ok, _} = Servers.add_member(ctx.server.id, alice.id, ctx.owner.id)

      roles =
        for name <- ~w(a b c) do
          {:ok, r} =
            Servers.create_role(ctx.server.id, ctx.owner.id, %{name: name, permissions: []})

          r
        end

      [a, b, c] = Enum.map(roles, & &1.id)
      :ok = Servers.assign_role(ctx.server.id, alice.id, a, ctx.owner.id)
      :ok = Servers.assign_role(ctx.server.id, alice.id, b, ctx.owner.id)

      assert {:error, :limit_exceeded} =
               Servers.assign_role(ctx.server.id, alice.id, c, ctx.owner.id)

      assert {:error, :limit_exceeded} =
               Servers.set_roles(ctx.server.id, alice.id, [a, b, c], ctx.owner.id)

      assert Enum.sort(Servers.member_for(ctx.server.id, alice.id).role_ids) == Enum.sort([a, b])
    end

    test "set_roles applies the whole diff in one transaction with one account event", ctx do
      alice = account("alice")
      {:ok, _} = Servers.add_member(ctx.server.id, alice.id, ctx.owner.id)

      {:ok, a} = Servers.create_role(ctx.server.id, ctx.owner.id, %{name: "a", permissions: []})
      {:ok, b} = Servers.create_role(ctx.server.id, ctx.owner.id, %{name: "b", permissions: []})
      :ok = Servers.assign_role(ctx.server.id, alice.id, a.id, ctx.owner.id)
      before = Enum.count(account_kinds(alice.id), &(&1 == "membership_updated"))

      assert {:ok, _member} = Servers.set_roles(ctx.server.id, alice.id, [b.id], ctx.owner.id)
      assert Servers.member_for(ctx.server.id, alice.id).role_ids == [b.id]
      assert Enum.count(account_kinds(alice.id), &(&1 == "membership_updated")) == before + 1

      # Repeating the current roles changes nothing and appends nothing.
      assert {:ok, _} = Servers.set_roles(ctx.server.id, alice.id, [b.id], ctx.owner.id)
      assert Enum.count(account_kinds(alice.id), &(&1 == "membership_updated")) == before + 1

      # A role the actor cannot manage fails the whole request; nothing is applied.
      mod = account("mod")
      {:ok, _} = Servers.add_member(ctx.server.id, mod.id, ctx.owner.id)

      assert {:error, :permission_denied} =
               Servers.set_roles(ctx.server.id, alice.id, [a.id], mod.id)

      assert Servers.member_for(ctx.server.id, alice.id).role_ids == [b.id]

      # An unknown role is NOT_FOUND.
      assert {:error, :not_found} =
               Servers.set_roles(ctx.server.id, alice.id, [Ecto.UUID.generate()], ctx.owner.id)
    end
  end

  describe "member-level changes" do
    @tag failure_injection: [authorization: 2]
    test "assigning a role bumps nothing and appears on that member's account log only", ctx do
      alice = account("alice")
      {:ok, _} = Servers.add_member(ctx.server.id, alice.id, ctx.owner.id)

      {:ok, role} =
        Servers.create_role(ctx.server.id, ctx.owner.id, %{name: "mods", permissions: []})

      before = acl_versions([ctx.general.id])

      :ok = Servers.assign_role(ctx.server.id, alice.id, role.id, ctx.owner.id)

      assert acl_versions([ctx.general.id]) == before
      assert "membership_updated" in account_kinds(alice.id)
      refute "membership_updated" in account_kinds(ctx.owner.id)
      assert Servers.member_for(ctx.server.id, alice.id).role_ids == [role.id]
    end

    test "kicking appends membership_removed and channel_removed for viewable channels", ctx do
      alice = account("alice")
      {:ok, _} = Servers.add_member(ctx.server.id, alice.id, ctx.owner.id)
      ViewerDiff.diff_channel(ctx.general.id)
      ViewerDiff.diff_channel(ctx.random.id)

      :ok = Servers.remove_member(ctx.server.id, alice.id, ctx.owner.id, "removed")

      kinds = account_kinds(alice.id)
      assert "membership_removed" in kinds
      assert Enum.count(kinds, &(&1 == "channel_removed")) == 2
      refute Servers.member_for(ctx.server.id, alice.id)
    end
  end

  describe "hierarchy" do
    setup ctx do
      mod = account("mod")
      {:ok, _} = Servers.add_member(ctx.server.id, mod.id, ctx.owner.id)

      {:ok, mod_role} =
        Servers.create_role(ctx.server.id, ctx.owner.id, %{
          name: "mod",
          position: 5,
          permissions: [:manage_roles, :manage_members, :send_messages]
        })

      {:ok, high_role} =
        Servers.create_role(ctx.server.id, ctx.owner.id, %{
          name: "admin",
          position: 1,
          permissions: [:manage_server]
        })

      {:ok, low_role} =
        Servers.create_role(ctx.server.id, ctx.owner.id, %{
          name: "helper",
          position: 9,
          permissions: []
        })

      :ok = Servers.assign_role(ctx.server.id, mod.id, mod_role.id, ctx.owner.id)
      %{mod: mod, mod_role: mod_role, high_role: high_role, low_role: low_role}
    end

    test "owner cannot be kicked, even by a moderator", ctx do
      assert {:error, :permission_denied} =
               Servers.remove_member(ctx.server.id, ctx.owner.id, ctx.mod.id, "removed")
    end

    @tag failure_injection: [authorization: 5]
    test "an administrator cannot kick the owner either", ctx do
      admin = account("administrator")
      {:ok, _} = Servers.add_member(ctx.server.id, admin.id, ctx.owner.id)

      {:ok, role} =
        Servers.create_role(ctx.server.id, ctx.owner.id, %{
          name: "administrators",
          position: 2,
          permissions: [:administrator]
        })

      :ok = Servers.assign_role(ctx.server.id, admin.id, role.id, ctx.owner.id)

      assert {:error, :permission_denied} =
               Servers.remove_member(ctx.server.id, ctx.owner.id, admin.id, "removed")

      # Administrator is still enough for everyone else.
      :ok = Servers.remove_member(ctx.server.id, ctx.mod.id, admin.id, "removed")
    end

    @tag failure_injection: [authorization: 6]
    test "cannot edit a role above your highest", ctx do
      assert {:error, :permission_denied} =
               Servers.update_role(ctx.server.id, ctx.high_role.id, ctx.mod.id, %{name: "x"})

      assert {:ok, _} =
               Servers.update_role(ctx.server.id, ctx.low_role.id, ctx.mod.id, %{name: "x"})
    end

    @tag failure_injection: [authorization: 6]
    test "cannot grant what you lack", ctx do
      assert {:error, :permission_denied} =
               Servers.update_role(ctx.server.id, ctx.low_role.id, ctx.mod.id, %{
                 permissions: [:manage_server]
               })

      assert {:ok, _} =
               Servers.update_role(ctx.server.id, ctx.low_role.id, ctx.mod.id, %{
                 permissions: [:send_messages]
               })
    end

    test "a moderator can kick someone below them but not above", ctx do
      low = account("low")
      {:ok, _} = Servers.add_member(ctx.server.id, low.id, ctx.owner.id)
      :ok = Servers.assign_role(ctx.server.id, low.id, ctx.low_role.id, ctx.owner.id)
      assert :ok = Servers.remove_member(ctx.server.id, low.id, ctx.mod.id, "removed")

      high = account("high")
      {:ok, _} = Servers.add_member(ctx.server.id, high.id, ctx.owner.id)
      :ok = Servers.assign_role(ctx.server.id, high.id, ctx.high_role.id, ctx.owner.id)

      assert {:error, :permission_denied} =
               Servers.remove_member(ctx.server.id, high.id, ctx.mod.id, "removed")
    end

    test "@everyone cannot be deleted", ctx do
      everyone = Repo.get_by!(Role, server_id: ctx.server.id, is_everyone: true)

      assert {:error, :permission_denied} =
               Servers.delete_role(ctx.server.id, everyone.id, ctx.owner.id)
    end
  end

  describe "viewer diff and private channels" do
    @tag failure_injection: [authorization: 9]
    test "making a channel private removes it from non-viewers and adds it for members of an allowed role",
         ctx do
      alice = account("alice")
      bob = account("bob")
      {:ok, _} = Servers.add_member(ctx.server.id, alice.id, ctx.owner.id)
      {:ok, _} = Servers.add_member(ctx.server.id, bob.id, ctx.owner.id)
      assert %{added: added} = ViewerDiff.diff_channel(ctx.general.id)
      assert Enum.sort(added) == Enum.sort([ctx.owner.id, alice.id, bob.id])

      everyone = Repo.get_by!(Role, server_id: ctx.server.id, is_everyone: true)

      {:ok, insiders} =
        Servers.create_role(ctx.server.id, ctx.owner.id, %{name: "insiders", permissions: []})

      :ok = Servers.assign_role(ctx.server.id, alice.id, insiders.id, ctx.owner.id)

      :ok =
        Servers.set_overwrite(ctx.general.id, ctx.owner.id, {:role, everyone.id}, [], [
          :view_channel
        ])

      :ok =
        Servers.set_overwrite(
          ctx.general.id,
          ctx.owner.id,
          {:role, insiders.id},
          [:view_channel],
          []
        )

      assert %{added: [], removed: [removed]} = ViewerDiff.diff_channel(ctx.general.id)
      assert removed == bob.id
      assert "channel_removed" in account_kinds(bob.id)
      refute "channel_removed" in account_kinds(alice.id)
      assert_enqueued(worker: ViewerDiff, args: %{server_id: ctx.server.id})
    end

    test "guests see nothing from @everyone", ctx do
      guest = account("guest")
      {:ok, _} = Servers.add_member(ctx.server.id, guest.id, ctx.owner.id, kind: "guest")
      refute guest.id in Servers.viewers(ctx.general.id)

      :ok =
        Servers.set_overwrite(
          ctx.general.id,
          ctx.owner.id,
          {:member, guest.id},
          [:view_channel],
          []
        )

      assert guest.id in Servers.viewers(ctx.general.id)
      refute guest.id in Servers.viewers(ctx.random.id)
    end
  end

  describe "snapshot cache" do
    @tag failure_injection: [authorization: 10]
    test "loads once per version and serves from ETS after", ctx do
      Cache.clear()
      {:ok, channel} = Repo.get(Channel, ctx.general.id) |> then(&{:ok, &1})
      assert {:ok, snap} = Cache.fetch(channel.id, channel.acl_version)
      size = Cache.size()
      assert {:ok, ^snap} = Cache.fetch(channel.id, channel.acl_version)
      assert Cache.size() == size
      assert {:error, :stale} = Cache.fetch(channel.id, channel.acl_version + 1)
    end

    test "resolves the owner to everything and a plain member to @everyone defaults", ctx do
      alice = account("alice")
      {:ok, _} = Servers.add_member(ctx.server.id, alice.id, ctx.owner.id)
      {:ok, snap} = Cache.fetch_current(ctx.general.id)

      assert Lumia.Authz.resolve(snap, Servers.member_for(ctx.server.id, ctx.owner.id)) ==
               Permission.all()

      assert snap
             |> Lumia.Authz.resolve(Servers.member_for(ctx.server.id, alice.id))
             |> Permission.has?(:view_channel)

      refute snap
             |> Lumia.Authz.resolve(Servers.member_for(ctx.server.id, alice.id))
             |> Permission.has?(:manage_server)
    end
  end
end
