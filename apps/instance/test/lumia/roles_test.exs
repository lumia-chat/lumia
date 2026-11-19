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
  alias Lumia.Servers
  alias Lumia.Servers.MemberAnnouncement
  alias Lumia.Servers.Role
  alias Lumia.V1
  alias Lumia.Workers.MembersUpdated
  alias Lumia.Workers.RolesUpdated

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
      assert Lumia.Authz.Permission.from_bits(updated.permissions) == [:view_channel]
    end

    test "@everyone is nobody's to give", ctx do
      everyone = Repo.get_by!(Role, server_id: ctx.server.id, is_everyone: true)
      alice = member(ctx, "alice")

      assert {:error, {:invalid_argument, "roles"}} ==
               Servers.set_roles(ctx.server.id, alice.id, [everyone.id], ctx.owner.id)
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
