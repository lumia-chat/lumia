defmodule Lumia.RolesTest do
  @moduledoc """
  Roles and titles as the clients manage them (architecture 7.5,
  authorization 3.5 and 6.4): the order is the instance's, names and
  colours are held to their rules, a title is its member's, and every
  member is told.
  """

  use Lumia.DataCase, async: true
  use Oban.Testing, repo: Lumia.Repo

  import Lumia.Fixtures

  alias Lumia.Accounts
  alias Lumia.Audit.Entry
  alias Lumia.Authz.Permission
  alias Lumia.Servers
  alias Lumia.Servers.MemberAnnouncement
  alias Lumia.Servers.MemberRole
  alias Lumia.Servers.Role
  alias Lumia.V1
  alias Lumia.Workers.MembersUpdated
  alias Lumia.Workers.RolesUpdated
  alias Lumia.Workers.ServerUpdated

  setup do
    owner = account("owner")
    {:ok, server} = Servers.create_server(owner.id, "Acme")
    %{owner: owner, server: server}
  end

  defp role(ctx, name, permissions \\ []) do
    {:ok, role} =
      Servers.create_role(ctx.server.id, ctx.owner.id, %{name: name, permissions: permissions})

    role
  end

  defp member(ctx, name, roles \\ []) do
    account = account(name)
    {:ok, _} = Servers.add_member(ctx.server.id, account.id, ctx.owner.id)
    for r <- roles, do: :ok = Servers.assign_role(ctx.server.id, account.id, r.id, ctx.owner.id)
    account
  end

  # The server's roles but @everyone, as {name, position}, in order.
  defp order(ctx) do
    for %Role{is_everyone: false} = r <- Servers.roles(ctx.server.id), do: {r.name, r.position}
  end

  defp names(ctx), do: Enum.map(order(ctx), &elem(&1, 0))

  # The roles a member holds.
  defp held(ctx, account) do
    Repo.all(
      from mr in MemberRole,
        where: mr.server_id == ^ctx.server.id and mr.account_id == ^account.id,
        select: mr.role_id
    )
  end

  defp audited(ctx, action) do
    Repo.aggregate(
      from(e in Entry, where: e.scope_id == ^ctx.server.id and e.action == ^action),
      :count
    )
  end

  defp run(worker) do
    for %Oban.Job{args: args} <- all_enqueued(worker: worker) do
      assert :ok = perform_job(worker, args)
    end
  end

  defp told(account, tag) do
    for %V1.AccountEvent{payload: {^tag, payload}} <- Accounts.events_after(account.id, 0),
        do: payload
  end

  describe "the order" do
    @tag failure_injection: [authorization: 11]
    test "a new role is placed last, above @everyone only, whatever is asked", ctx do
      first = role(ctx, "first")

      {:ok, second} =
        Servers.create_role(ctx.server.id, ctx.owner.id, %{name: "second", position: 0})

      assert first.position == 1 and second.position == 2

      assert [%Role{name: "first"}, %Role{name: "second"}, %Role{is_everyone: true}] =
               Servers.roles(ctx.server.id)
    end

    @tag failure_injection: [authorization: 11]
    test "an update never moves a role", ctx do
      first = role(ctx, "first")
      role(ctx, "second")

      {:ok, updated} =
        Servers.update_role(ctx.server.id, first.id, ctx.owner.id, %{name: "one", position: 9})

      assert updated.position == 1
      assert names(ctx) == ["one", "second"]
    end

    @tag failure_injection: [authorization: 12]
    test "the owner reorders, and the roles are numbered from 1", ctx do
      [a, b, c] = for name <- ~w(a b c), do: role(ctx, name)

      assert {:ok, roles} = Servers.reorder_roles(ctx.server.id, ctx.owner.id, [c.id, a.id, b.id])

      assert [%Role{name: "c"}, %Role{name: "a"}, %Role{name: "b"}, %Role{is_everyone: true}] =
               roles

      assert order(ctx) == [{"c", 1}, {"a", 2}, {"b", 3}]
      assert audited(ctx, "role.reorder") == 1
      assert_enqueued(worker: RolesUpdated, args: %{server_id: ctx.server.id})
    end

    test "the order held is answered and nothing is written", ctx do
      [a, b] = for name <- ~w(a b), do: role(ctx, name)

      assert {:ok, _} = Servers.reorder_roles(ctx.server.id, ctx.owner.id, [a.id, b.id])
      assert audited(ctx, "role.reorder") == 0
    end

    @tag failure_injection: [authorization: 12]
    test "an order that is not the server's roles, each once, is refused", ctx do
      [a, b] = for name <- ~w(a b), do: role(ctx, name)
      everyone = Repo.get_by!(Role, server_id: ctx.server.id, is_everyone: true)
      refused = {:error, {:invalid_argument, "role_ids"}}

      assert refused == Servers.reorder_roles(ctx.server.id, ctx.owner.id, [a.id])
      assert refused == Servers.reorder_roles(ctx.server.id, ctx.owner.id, [a.id, a.id])

      assert refused ==
               Servers.reorder_roles(ctx.server.id, ctx.owner.id, [b.id, a.id, everyone.id])

      assert refused ==
               Servers.reorder_roles(ctx.server.id, ctx.owner.id, [
                 b.id,
                 a.id,
                 Ecto.UUID.generate()
               ])

      assert names(ctx) == ["a", "b"]
    end

    @tag failure_injection: [authorization: 12]
    test "nothing at or above the actor's highest moves, and nothing is lifted there", ctx do
      top = role(ctx, "top")
      mods = role(ctx, "mods", [:manage_roles])
      [x, y] = for name <- ~w(x y), do: role(ctx, name)
      mod = member(ctx, "mod", [mods])

      for wanted <- [
            # A role lifted above the actor's own.
            [top.id, x.id, mods.id, y.id],
            # The actor's own role moved.
            [top.id, x.id, y.id, mods.id],
            # A role above the actor moved.
            [mods.id, top.id, x.id, y.id]
          ] do
        assert {:error, :permission_denied} ==
                 Servers.reorder_roles(ctx.server.id, mod.id, wanted)
      end

      assert names(ctx) == ~w(top mods x y)

      assert {:ok, _} =
               Servers.reorder_roles(ctx.server.id, mod.id, [top.id, mods.id, y.id, x.id])

      assert names(ctx) == ~w(top mods y x)
    end

    test "it takes MANAGE_ROLES, and holding no role reorders nothing", ctx do
      [a, b] = for name <- ~w(a b), do: role(ctx, name)
      plain = member(ctx, "plain")

      assert {:error, :permission_denied} ==
               Servers.reorder_roles(ctx.server.id, plain.id, [b.id, a.id])

      everyone = Repo.get_by!(Role, server_id: ctx.server.id, is_everyone: true)

      {:ok, _} =
        Servers.update_role(ctx.server.id, everyone.id, ctx.owner.id, %{
          permissions: [:manage_roles]
        })

      assert {:error, :permission_denied} ==
               Servers.reorder_roles(ctx.server.id, plain.id, [b.id, a.id])
    end

    test "someone with MANAGE_ROLES makes a role, and it lands below their own", ctx do
      mods = role(ctx, "mods", [:manage_roles])
      mod = member(ctx, "mod", [mods])

      assert {:ok, made} = Servers.create_role(ctx.server.id, mod.id, %{name: "made"})
      assert made.position > mods.position
      assert {:ok, _} = Servers.update_role(ctx.server.id, made.id, mod.id, %{name: "remade"})
    end
  end

  describe "a role" do
    test "its name is trimmed, 1 to 32 characters, on one line", ctx do
      assert {:ok, %Role{name: "Design"}} =
               Servers.create_role(ctx.server.id, ctx.owner.id, %{name: "  Design  "})

      for name <- ["", "   ", String.duplicate("a", 33), "two\nlines", nil] do
        assert {:error, {:invalid_argument, "name"}} ==
                 Servers.create_role(ctx.server.id, ctx.owner.id, %{name: name})
      end

      assert {:ok, _} =
               Servers.create_role(ctx.server.id, ctx.owner.id, %{name: String.duplicate("a", 32)})
    end

    @tag failure_injection: [authorization: 13]
    test "no two roles of a server share a name, whatever its capitals", ctx do
      design = role(ctx, "Design")
      other = role(ctx, "Other")

      assert {:error, :already_exists} ==
               Servers.create_role(ctx.server.id, ctx.owner.id, %{name: "design"})

      assert {:error, :already_exists} ==
               Servers.update_role(ctx.server.id, other.id, ctx.owner.id, %{name: "DESIGN"})

      # Its own name in other capitals is its own.
      assert {:ok, %Role{name: "DESIGN"}} =
               Servers.update_role(ctx.server.id, design.id, ctx.owner.id, %{name: "DESIGN"})

      {:ok, elsewhere} = Servers.create_server(ctx.owner.id, "Elsewhere")
      assert {:ok, _} = Servers.create_role(elsewhere.id, ctx.owner.id, %{name: "Design"})
    end

    @tag failure_injection: [authorization: 17]
    test "its name is what it looks like", ctx do
      # A line separator, a space of no width, a mark that turns the text
      # round, and a name of nothing but a joiner.
      for name <- ["Admin\u2028Owner", "mods\u200B", "\u202Enimda", "\u200B", "\u200D"] do
        assert {:error, {:invalid_argument, "name"}} ==
                 Servers.create_role(ctx.server.id, ctx.owner.id, %{name: name})
      end

      # A joiner inside an emoji is how the emoji is written.
      assert {:ok, %Role{name: "Family 👨‍👩‍👧"}} =
               Servers.create_role(ctx.server.id, ctx.owner.id, %{name: "Family 👨‍👩‍👧"})
    end

    test "everyone's name is no role's", ctx do
      for name <- ["everyone", "@everyone", "@Everyone"] do
        assert {:error, {:invalid_argument, "name"}} ==
                 Servers.create_role(ctx.server.id, ctx.owner.id, %{name: name})
      end
    end

    @tag failure_injection: [authorization: 13]
    test "its colour is a tone or none", ctx do
      assert {:ok, %Role{color: "mint"} = role} =
               Servers.create_role(ctx.server.id, ctx.owner.id, %{name: "a", color: "mint"})

      for color <- ["#ff0000", "red", "Mint"] do
        assert {:error, {:invalid_argument, "color"}} ==
                 Servers.update_role(ctx.server.id, role.id, ctx.owner.id, %{color: color})
      end

      assert {:ok, %Role{color: nil}} =
               Servers.update_role(ctx.server.id, role.id, ctx.owner.id, %{color: ""})
    end

    test "it is shown as a tag unless that is turned off", ctx do
      assert %Role{hide_tag: false} = role = role(ctx, "a")

      assert {:ok, %Role{hide_tag: true}} =
               Servers.update_role(ctx.server.id, role.id, ctx.owner.id, %{hide_tag: true})
    end

    test "of @everyone only the permissions change", ctx do
      everyone = Repo.get_by!(Role, server_id: ctx.server.id, is_everyone: true)

      assert {:ok, updated} =
               Servers.update_role(ctx.server.id, everyone.id, ctx.owner.id, %{
                 name: "all",
                 color: "mint",
                 hide_tag: true,
                 permissions: [:view_channel]
               })

      assert %Role{name: "@everyone", color: nil, hide_tag: false, position: 1_000_000} = updated
      assert Permission.from_bits(updated.permissions) == [:view_channel]
    end

    test "@everyone is nobody's to give", ctx do
      everyone = Repo.get_by!(Role, server_id: ctx.server.id, is_everyone: true)
      alice = member(ctx, "alice")

      assert {:error, {:invalid_argument, "roles"}} ==
               Servers.set_roles(ctx.server.id, alice.id, [everyone.id], ctx.owner.id)
    end
  end

  describe "a role's permissions" do
    test "are switched on and off, the rest as they are", ctx do
      mods = role(ctx, "mods", [:manage_messages, :create_invites])

      assert {:ok, updated} =
               Servers.update_role(ctx.server.id, mods.id, ctx.owner.id, %{
                 grant: [:manage_channels],
                 revoke: [:create_invites]
               })

      assert Enum.sort(Permission.from_bits(updated.permissions)) ==
               [:manage_channels, :manage_messages]

      # A name alone leaves them alone.
      assert {:ok, renamed} =
               Servers.update_role(ctx.server.id, mods.id, ctx.owner.id, %{name: "Mods"})

      assert renamed.permissions == updated.permissions
    end

    test "one switched both on and off is refused", ctx do
      mods = role(ctx, "mods")

      assert {:error, {:invalid_argument, "permissions"}} ==
               Servers.update_role(ctx.server.id, mods.id, ctx.owner.id, %{
                 grant: [:manage_channels],
                 revoke: [:manage_channels]
               })
    end

    @tag failure_injection: [authorization: 18]
    test "a change another made is not undone by one that does not name it", ctx do
      # Two editors opened on the role as it was: one takes Administrator
      # away, the other then saves a new name and one more permission.
      admins = role(ctx, "admins", [:administrator])

      {:ok, _} =
        Servers.update_role(ctx.server.id, admins.id, ctx.owner.id, %{revoke: [:administrator]})

      {:ok, saved} =
        Servers.update_role(ctx.server.id, admins.id, ctx.owner.id, %{
          name: "Leads",
          grant: [:create_invites]
        })

      assert Permission.from_bits(saved.permissions) == [:create_invites]
    end

    test "nobody switches on what they do not hold", ctx do
      mods = role(ctx, "mods", [:manage_roles])
      low = role(ctx, "low")
      mod = member(ctx, "mod", [mods])

      assert {:error, :permission_denied} ==
               Servers.update_role(ctx.server.id, low.id, mod.id, %{grant: [:manage_server]})

      assert {:ok, _} =
               Servers.update_role(ctx.server.id, low.id, mod.id, %{grant: [:manage_roles]})
    end
  end

  describe "giving and taking a member's roles" do
    test "leaves the member's other roles as they are", ctx do
      mods = role(ctx, "mods")
      events = role(ctx, "events")
      design = role(ctx, "design")
      alice = member(ctx, "alice", [mods, events])

      assert {:ok, _} =
               Servers.give_and_take_roles(
                 ctx.server.id,
                 alice.id,
                 [design.id],
                 [mods.id],
                 ctx.owner.id
               )

      assert Enum.sort(held(ctx, alice)) ==
               Enum.sort([events.id, design.id])
    end

    @tag failure_injection: [authorization: 18]
    test "a role given by another since the list was read stays", ctx do
      # One admin gives Events while another, whose list still shows Mods
      # alone, takes Mods away: the member is left with Events.
      mods = role(ctx, "mods")
      events = role(ctx, "events")
      alice = member(ctx, "alice", [mods])

      {:ok, _} =
        Servers.give_and_take_roles(ctx.server.id, alice.id, [events.id], [], ctx.owner.id)

      {:ok, _} = Servers.give_and_take_roles(ctx.server.id, alice.id, [], [mods.id], ctx.owner.id)

      assert held(ctx, alice) == [events.id]
    end

    test "one held is not given again, and one not held is not taken", ctx do
      mods = role(ctx, "mods")
      gone = role(ctx, "gone")
      alice = member(ctx, "alice", [mods])
      :ok = Servers.delete_role(ctx.server.id, gone.id, ctx.owner.id)
      before = length(told(alice, :membership_updated))

      assert {:ok, _} =
               Servers.give_and_take_roles(
                 ctx.server.id,
                 alice.id,
                 [mods.id],
                 [gone.id],
                 ctx.owner.id
               )

      assert length(told(alice, :membership_updated)) == before
      assert held(ctx, alice) == [mods.id]
    end

    test "a role in both, one the actor does not manage, and one that is gone are refused",
         ctx do
      top = role(ctx, "top")
      mods = role(ctx, "mods", [:manage_roles])
      low = role(ctx, "low")
      gone = role(ctx, "gone")
      mod = member(ctx, "mod", [mods])
      alice = member(ctx, "alice", [top])
      :ok = Servers.delete_role(ctx.server.id, gone.id, ctx.owner.id)

      assert {:error, {:invalid_argument, "roles"}} ==
               Servers.give_and_take_roles(ctx.server.id, alice.id, [low.id], [low.id], mod.id)

      assert {:error, :permission_denied} ==
               Servers.give_and_take_roles(ctx.server.id, alice.id, [low.id], [top.id], mod.id)

      assert {:error, :not_found} ==
               Servers.give_and_take_roles(ctx.server.id, alice.id, [gone.id], [], mod.id)

      # Refused whole: the role it could have given was not.
      assert held(ctx, alice) == [top.id]
      assert {:ok, _} = Servers.give_and_take_roles(ctx.server.id, alice.id, [low.id], [], mod.id)
    end
  end

  describe "a title" do
    @tag failure_injection: [authorization: 15]
    test "is the member's own to set and to clear", ctx do
      alice = member(ctx, "alice")

      assert {:ok, %{title: "Staff Engineer"}} =
               Servers.update_membership(ctx.server.id, alice.id, %{title: "  Staff Engineer "})

      assert audited(ctx, "member.title") == 1

      assert [%V1.MembershipUpdated{member: %V1.Member{title: "Staff Engineer"}}] =
               told(alice, :membership_updated)

      assert {:ok, %{title: nil}} =
               Servers.update_membership(ctx.server.id, alice.id, %{title: ""})
    end

    test "a switch changed leaves the title as it is", ctx do
      alice = member(ctx, "alice")
      {:ok, _} = Servers.update_membership(ctx.server.id, alice.id, %{title: "Designer"})

      assert {:ok, %{title: "Designer", server_dms_off: true}} =
               Servers.update_membership(ctx.server.id, alice.id, %{server_dms_off: true})

      assert audited(ctx, "member.title") == 1
    end

    test "is up to 32 characters on one line", ctx do
      alice = member(ctx, "alice")

      for title <- [String.duplicate("a", 33), "two\nlines", 7] do
        assert {:error, {:invalid_argument, "title"}} ==
                 Servers.update_membership(ctx.server.id, alice.id, %{title: title})
      end
    end

    @tag failure_injection: [authorization: 15]
    test "someone else's takes an actor who could kick them", ctx do
      top = role(ctx, "top")
      mods = role(ctx, "mods", [:manage_members])
      low = role(ctx, "low")
      mod = member(ctx, "mod", [mods])
      above = member(ctx, "above", [top])
      below = member(ctx, "below", [low])
      plain = member(ctx, "plain")

      assert {:ok, %{title: "Helper"}} =
               Servers.set_title(ctx.server.id, below.id, mod.id, "Helper")

      assert {:ok, %{title: "New"}} = Servers.set_title(ctx.server.id, plain.id, mod.id, "New")

      for target <- [above, ctx.owner] do
        assert {:error, :permission_denied} ==
                 Servers.set_title(ctx.server.id, target.id, mod.id, "Nope")
      end

      # Without MANAGE_MEMBERS, nobody's but their own.
      assert {:error, :permission_denied} ==
               Servers.set_title(ctx.server.id, below.id, plain.id, "Nope")

      assert {:ok, %{title: "Mine"}} =
               Servers.set_title(ctx.server.id, plain.id, plain.id, "Mine")

      assert {:ok, %{title: "Founder"}} =
               Servers.set_title(ctx.server.id, ctx.owner.id, ctx.owner.id, "Founder")

      # The member whose title someone else set hears of it on its own log.
      assert %V1.MembershipUpdated{member: %V1.Member{title: "Helper"}} =
               List.last(told(below, :membership_updated))
    end

    @tag failure_injection: [authorization: 17]
    test "is what it looks like", ctx do
      alice = member(ctx, "alice")

      for title <- ["Staff\u2028Engineer", "\u202EreenignE", "\u200B"] do
        assert {:error, {:invalid_argument, "title"}} ==
                 Servers.update_membership(ctx.server.id, alice.id, %{title: title})
      end
    end

    @tag failure_injection: [authorization: 19]
    test "a member changes its own only so often, a manager's change aside", ctx do
      alice = member(ctx, "alice")

      # Ten in an hour (authorization 10), by either route.
      for n <- 1..9 do
        {:ok, _} = Servers.update_membership(ctx.server.id, alice.id, %{title: "Title #{n}"})
      end

      {:ok, _} = Servers.set_title(ctx.server.id, alice.id, alice.id, "Title 10")

      assert {:error, :rate_limited} ==
               Servers.update_membership(ctx.server.id, alice.id, %{title: "One more"})

      assert {:error, :rate_limited} ==
               Servers.set_title(ctx.server.id, alice.id, alice.id, "One more")

      # The title held, and a switch, are no change of title.
      assert {:ok, %{title: "Title 10", server_dms_off: true}} =
               Servers.update_membership(ctx.server.id, alice.id, %{
                 title: "Title 10",
                 server_dms_off: true
               })

      # The owner's change of it is the owner's, and is not counted.
      assert {:ok, %{title: "Four"}} =
               Servers.set_title(ctx.server.id, alice.id, ctx.owner.id, "Four")

      # Nor is what alice did in another server.
      {:ok, elsewhere} = Servers.create_server(ctx.owner.id, "Elsewhere")
      {:ok, _} = Servers.add_member(elsewhere.id, alice.id, ctx.owner.id)
      assert {:ok, _} = Servers.update_membership(elsewhere.id, alice.id, %{title: "One"})
    end

    test "a guest has one too", ctx do
      guest = account("guest")
      {:ok, _} = Servers.add_member(ctx.server.id, guest.id, ctx.owner.id, kind: "guest")

      assert {:ok, %{title: "Contractor"}} =
               Servers.update_membership(ctx.server.id, guest.id, %{title: "Contractor"})
    end

    test "the title already held changes nothing", ctx do
      alice = member(ctx, "alice")
      {:ok, _} = Servers.set_title(ctx.server.id, alice.id, ctx.owner.id, "Designer")
      {:ok, _} = Servers.set_title(ctx.server.id, alice.id, ctx.owner.id, "Designer")

      assert audited(ctx, "member.title") == 1
      assert length(told(alice, :membership_updated)) == 1
    end
  end

  describe "what every member is told" do
    @tag failure_injection: [authorization: 12]
    test "a server's roles, once for a burst of changes", ctx do
      alice = member(ctx, "alice")
      a = role(ctx, "a")
      b = role(ctx, "b")
      {:ok, _} = Servers.reorder_roles(ctx.server.id, ctx.owner.id, [b.id, a.id])
      {:ok, _} = Servers.update_role(ctx.server.id, a.id, ctx.owner.id, %{color: "sky"})
      :ok = Servers.delete_role(ctx.server.id, b.id, ctx.owner.id)

      assert [_one] = all_enqueued(worker: RolesUpdated)
      run(RolesUpdated)

      for account <- [alice, ctx.owner] do
        assert [%V1.RolesUpdated{server_id: server_id, roles: roles}] =
                 told(account, :roles_updated)

        assert Lumia.Proto.from_uuid(server_id) == ctx.server.id

        assert [
                 %V1.Role{name: "a", color: "sky", position: 2, is_everyone: false},
                 %V1.Role{is_everyone: true}
               ] = roles
      end
    end

    @tag failure_injection: [authorization: 14]
    test "a member's roles: at once to the member, and to everyone after the job", ctx do
      alice = member(ctx, "alice")
      bob = member(ctx, "bob")
      mods = role(ctx, "mods")

      {:ok, _} = Servers.set_roles(ctx.server.id, alice.id, [mods.id], ctx.owner.id)

      assert [%V1.MembershipUpdated{}] = told(alice, :membership_updated)
      assert told(bob, :members_updated) == []

      run(MembersUpdated)

      for account <- [alice, bob, ctx.owner] do
        assert [%V1.MembersUpdated{members: [member]}] = told(account, :members_updated)
        assert Lumia.Proto.from_uuid(member.account_id) == alice.id
        assert Enum.map(member.role_ids, &Lumia.Proto.from_uuid/1) == [mods.id]
        assert member.permissions == []
      end

      assert Repo.aggregate(MemberAnnouncement, :count) == 0
    end

    test "titles and roles of several members are one event, each member once", ctx do
      alice = member(ctx, "alice")
      bob = member(ctx, "bob")
      mods = role(ctx, "mods")

      {:ok, _} = Servers.update_membership(ctx.server.id, alice.id, %{title: "Designer"})
      :ok = Servers.assign_role(ctx.server.id, alice.id, mods.id, ctx.owner.id)
      {:ok, _} = Servers.set_title(ctx.server.id, bob.id, ctx.owner.id, "Engineer")

      assert [_one] = all_enqueued(worker: MembersUpdated)
      run(MembersUpdated)

      assert [%V1.MembersUpdated{members: members}] = told(ctx.owner, :members_updated)

      assert members |> Enum.map(& &1.title) |> Enum.sort() == ["Designer", "Engineer"]
    end

    test "a guest is told only of the members it shares a channel with", ctx do
      {:ok, general} = Servers.create_channel(ctx.server.id, ctx.owner.id, "general")
      alice = member(ctx, "alice")
      bob = member(ctx, "bob")
      guest = account("guest")
      {:ok, _} = Servers.add_member(ctx.server.id, guest.id, ctx.owner.id, kind: "guest")
      {:ok, _} = Lumia.Channels.enter_quietly(general, guest.id)
      # A newcomer arrives in #general, the server's first channel: Bob leaves it.
      :ok = Lumia.Channels.leave(general.id, bob.id)

      {:ok, _} = Servers.set_title(ctx.server.id, alice.id, ctx.owner.id, "Designer")
      {:ok, _} = Servers.set_title(ctx.server.id, bob.id, ctx.owner.id, "Engineer")
      run(MembersUpdated)

      # Everyone else hears of both; the guest of Alice alone.
      assert [%V1.MembersUpdated{members: both}] = told(ctx.owner, :members_updated)
      assert both |> Enum.map(& &1.title) |> Enum.sort() == ["Designer", "Engineer"]
      assert [%V1.MembersUpdated{members: [seen]}] = told(guest, :members_updated)
      assert Lumia.Proto.from_uuid(seen.account_id) == alice.id

      # A change only to someone it does not see appends nothing to its log.
      {:ok, _} = Servers.set_title(ctx.server.id, bob.id, ctx.owner.id, "Lead")
      run(MembersUpdated)
      assert length(told(ctx.owner, :members_updated)) == 2
      assert length(told(guest, :members_updated)) == 1

      # The server's roles are not the member list: a guest hears of them as anyone does.
      role(ctx, "mods")
      run(RolesUpdated)
      assert [%V1.RolesUpdated{}] = told(guest, :roles_updated)
    end

    test "a server that is gone is told nothing, by any of the jobs" do
      gone = Ecto.UUID.generate()

      for worker <- [RolesUpdated, MembersUpdated, ServerUpdated] do
        assert :ok = perform_job(worker, %{"server_id" => gone})
      end
    end

    test "one who left before the job ran is left out", ctx do
      alice = member(ctx, "alice")
      {:ok, _} = Servers.update_membership(ctx.server.id, alice.id, %{title: "Designer"})
      :ok = Servers.remove_member(ctx.server.id, alice.id, alice.id, "left")

      run(MembersUpdated)

      assert told(ctx.owner, :members_updated) == []
      assert Repo.aggregate(MemberAnnouncement, :count) == 0
    end

    @tag failure_injection: [authorization: 16]
    test "someone who joins gets the server's roles as they are", ctx do
      role(ctx, "mods", [:manage_messages])
      alice = member(ctx, "alice")

      assert [%V1.MembershipAdded{roles: roles}] = told(alice, :membership_added)

      assert [
               %V1.Role{name: "mods", permissions: [:MANAGE_MESSAGES]},
               %V1.Role{is_everyone: true}
             ] =
               roles
    end

    @tag failure_injection: [authorization: 16]
    test "the account snapshot carries the roles of every server the account is in", ctx do
      role(ctx, "mods")
      {:ok, other} = Servers.create_server(account("other").id, "Other")
      {:ok, _} = Servers.create_role(other.id, other.owner_account_id, %{name: "theirs"})

      assert %V1.AccountSnapshot{roles: roles} = Lumia.Snapshots.account(ctx.owner.id)
      assert Enum.map(roles, & &1.name) == ["mods", "@everyone"]
    end
  end
end
