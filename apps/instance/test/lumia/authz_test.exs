defmodule Lumia.AuthzTest do
  use ExUnit.Case, async: true
  use ExUnitProperties

  alias Lumia.Authz
  alias Lumia.Authz.Permission
  alias Lumia.Authz.Snapshot
  alias Lumia.AuthzReference

  @everyone "00000000-0000-0000-0000-00000000000e"
  @role_ids Enum.map(1..4, &"00000000-0000-0000-0000-00000000000#{&1}")
  @account "00000000-0000-0000-0000-0000000000aa"

  defp bits_gen do
    gen all(names <- list_of(member_of(Permission.names()), max_length: 6)) do
      Permission.to_bits(names)
    end
  end

  defp overwrite_gen do
    gen(all(allow <- bits_gen(), deny <- bits_gen(), do: %{allow: allow, deny: deny}))
  end

  defp snapshot_gen do
    gen all(
          everyone_perms <- bits_gen(),
          role_perms <- list_of(bits_gen(), length: 4),
          everyone_ow <- one_of([constant(nil), overwrite_gen()]),
          role_ows <- list_of(one_of([constant(nil), overwrite_gen()]), length: 4),
          member_ow <- one_of([constant(nil), overwrite_gen()])
        ) do
      roles =
        [{@everyone, %{permissions: everyone_perms, position: 999}}] ++
          Enum.zip_with(@role_ids, Enum.with_index(role_perms), fn id, {perms, i} ->
            {id, %{permissions: perms, position: i}}
          end)

      overwrites =
        [{{:role, @everyone}, everyone_ow}, {{:member, @account}, member_ow}] ++
          Enum.zip_with(@role_ids, role_ows, fn id, ow -> {{:role, id}, ow} end)

      %Snapshot{
        channel_id: "c",
        server_id: "s",
        acl_version: 1,
        everyone_role_id: @everyone,
        roles: Map.new(roles),
        overwrites: overwrites |> Enum.reject(fn {_, ow} -> is_nil(ow) end) |> Map.new()
      }
    end
  end

  defp member_gen do
    gen all(
          kind <- member_of([:member, :guest]),
          role_ids <- list_of(member_of(@role_ids), max_length: 4),
          owner? <- frequency([{9, constant(false)}, {1, constant(true)}])
        ) do
      %{account_id: @account, kind: kind, role_ids: Enum.uniq(role_ids), owner?: owner?}
    end
  end

  property "the bitfield implementation agrees with the MapSet reference" do
    check all(snapshot <- snapshot_gen(), member <- member_gen(), max_runs: 500) do
      got = snapshot |> Authz.resolve(member) |> Permission.from_bits() |> MapSet.new()
      want = AuthzReference.resolve(snapshot, member)

      assert MapSet.equal?(got, want),
             "got #{inspect(MapSet.to_list(got))}, want #{inspect(MapSet.to_list(want))}"
    end
  end

  property "the owner always resolves everything" do
    check all(snapshot <- snapshot_gen(), member <- member_gen()) do
      assert Authz.resolve(snapshot, %{member | owner?: true}) == Permission.all()
    end
  end

  property "a guest never gains anything from @everyone" do
    check all(snapshot <- snapshot_gen(), member <- member_gen()) do
      guest = %{member | kind: :guest, owner?: false, role_ids: []}

      assert Authz.resolve(snapshot, guest) == 0 or
               Map.has_key?(snapshot.overwrites, {:member, @account})
    end
  end

  property "Administrator in base ignores overwrites" do
    check all(snapshot <- snapshot_gen(), member <- member_gen()) do
      admin_role = hd(@role_ids)

      roles =
        Map.put(snapshot.roles, admin_role, %{
          permissions: Permission.bit(:administrator),
          position: 0
        })

      member = %{member | kind: :member, owner?: false, role_ids: [admin_role]}
      assert Authz.resolve(%{snapshot | roles: roles}, member) == Permission.all()
    end
  end

  describe "layers" do
    setup do
      role = hd(@role_ids)

      snapshot = %Snapshot{
        channel_id: "c",
        acl_version: 1,
        everyone_role_id: @everyone,
        roles: %{
          @everyone => %{
            permissions: Permission.to_bits([:view_channel, :send_messages]),
            position: 99
          },
          role => %{permissions: Permission.to_bits([:attach_files]), position: 1}
        },
        overwrites: %{}
      }

      %{
        snapshot: snapshot,
        role: role,
        member: %{account_id: @account, kind: :member, role_ids: [role], owner?: false}
      }
    end

    test "deny beats allow within a layer, later layers override earlier", ctx do
      ow = %{{:role, @everyone} => %{allow: 0, deny: Permission.bit(:send_messages)}}

      assert ctx.snapshot
             |> Map.put(:overwrites, ow)
             |> Authz.resolve(ctx.member)
             |> Permission.from_bits() == [:attach_files, :view_channel]

      ow = Map.put(ow, {:member, @account}, %{allow: Permission.bit(:send_messages), deny: 0})

      assert ctx.snapshot
             |> Map.put(:overwrites, ow)
             |> Authz.resolve(ctx.member)
             |> Permission.has?(:send_messages)
    end

    test "an overwrite with a bit in both allow and deny is normalized to deny", _ctx do
      ow = %{
        allow: Permission.to_bits([:view_channel, :send_messages]),
        deny: Permission.bit(:view_channel)
      }

      assert Authz.normalize_overwrite(ow) == %{
               allow: Permission.bit(:send_messages),
               deny: Permission.bit(:view_channel)
             }
    end

    test "private? reads the @everyone overwrite", ctx do
      refute Authz.private?(ctx.snapshot)
      ow = %{{:role, @everyone} => %{allow: 0, deny: Permission.bit(:view_channel)}}
      assert Authz.private?(%{ctx.snapshot | overwrites: ow})
    end
  end

  describe "management hierarchy" do
    @mod %{
      account_id: "m",
      owner?: false,
      server_permissions: Permission.to_bits([:manage_roles, :manage_members, :send_messages]),
      highest_position: 2
    }

    test "can manage only roles below your highest" do
      assert Authz.can_manage_role?(@mod, %{position: 3})
      refute Authz.can_manage_role?(@mod, %{position: 2})
      refute Authz.can_manage_role?(@mod, %{position: 1})
      assert Authz.can_manage_role?(%{@mod | owner?: true}, %{position: 0})
    end

    test "cannot grant what you lack" do
      assert Authz.can_grant?(@mod, Permission.to_bits([:send_messages]))
      refute Authz.can_grant?(@mod, Permission.to_bits([:send_messages, :manage_server]))

      assert Authz.can_grant?(
               %{@mod | server_permissions: Permission.bit(:administrator)},
               Permission.all()
             )
    end

    test "owner can never be removed; targets below you can" do
      refute Authz.can_remove_member?(%{@mod | owner?: true}, %{
               owner?: true,
               highest_position: nil
             })

      assert Authz.can_remove_member?(@mod, %{owner?: false, highest_position: 5})
      assert Authz.can_remove_member?(@mod, %{owner?: false, highest_position: nil})
      refute Authz.can_remove_member?(@mod, %{owner?: false, highest_position: 2})

      refute Authz.can_remove_member?(%{@mod | highest_position: nil}, %{
               owner?: false,
               highest_position: 5
             })
    end
  end

  test "Permission round-trips names and bits" do
    assert Permission.names() |> Permission.to_bits() == Permission.all()
    assert Permission.all() |> Permission.from_bits() == Enum.sort(Permission.names())
    assert Permission.from_bits(Permission.bit(:view_channel)) == [:view_channel]
  end
end
