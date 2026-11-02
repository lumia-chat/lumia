defmodule Lumia.Authz do
  @moduledoc """
  Permission resolution (docs/specs/authorization.md section 3).

  `resolve/2` is a pure function of an ACL snapshot and a member. It is
  implemented over bitfields and property-tested against the `MapSet`
  reference in `test/support/authz_reference.ex`.
  """

  import Bitwise

  alias Lumia.Authz.Permission
  alias Lumia.Authz.Snapshot

  @doc """
  Resolves a member's permissions in the snapshot's channel. Returns a
  bitfield; use `Lumia.Authz.Permission.from_bits/1` for names.

      1. owner                        → ALL
      2. guest → ∅; member → ∪ roles including @everyone
      3. ADMINISTRATOR in base        → ALL
      4. @everyone overwrite          (guests skip)
      5. role overwrites, aggregated  (deny ∪ then allow ∪)
      6. member overwrite
  """
  def resolve(%Snapshot{}, %{owner?: true}), do: Permission.all()

  def resolve(%Snapshot{} = snapshot, member) do
    base = base(snapshot, member)

    if Permission.has?(base, :administrator) do
      Permission.all()
    else
      base
      |> apply_everyone(snapshot, member)
      |> apply_roles(snapshot, member)
      |> apply_member(snapshot, member)
    end
  end

  defp base(_snapshot, %{kind: :guest}), do: 0

  defp base(snapshot, %{role_ids: role_ids}) do
    everyone = role_permissions(snapshot, snapshot.everyone_role_id)
    Enum.reduce(role_ids, everyone, fn id, acc -> acc ||| role_permissions(snapshot, id) end)
  end

  defp role_permissions(snapshot, role_id) do
    case Map.get(snapshot.roles, role_id) do
      nil -> 0
      role -> role.permissions
    end
  end

  defp apply_everyone(bits, _snapshot, %{kind: :guest}), do: bits

  defp apply_everyone(bits, snapshot, _member) do
    apply_overwrite(bits, Map.get(snapshot.overwrites, {:role, snapshot.everyone_role_id}))
  end

  defp apply_roles(bits, snapshot, %{role_ids: role_ids}) do
    {deny, allow} =
      Enum.reduce(role_ids, {0, 0}, fn id, {d, a} ->
        case Map.get(snapshot.overwrites, {:role, id}) do
          nil -> {d, a}
          ow -> {d ||| ow.deny, a ||| ow.allow}
        end
      end)

    (bits &&& bnot(deny)) ||| allow
  end

  defp apply_member(bits, snapshot, %{account_id: account_id}) do
    apply_overwrite(bits, Map.get(snapshot.overwrites, {:member, account_id}))
  end

  defp apply_overwrite(bits, nil), do: bits
  defp apply_overwrite(bits, %{allow: allow, deny: deny}), do: (bits &&& bnot(deny)) ||| allow

  @doc "Whether the member resolves the named permission."
  def can?(snapshot, member, permission),
    do: Permission.has?(resolve(snapshot, member), permission)

  @doc """
  Clears from `allow` any bit present in `deny`, so stored overwrites never
  overlap and the same-layer rule is unambiguous. Applied on every write.
  """
  def normalize_overwrite(%{allow: allow, deny: deny}),
    do: %{allow: allow &&& bnot(deny), deny: deny}

  @doc "A channel is private when its @everyone overwrite denies VIEW_CHANNEL."
  def private?(%Snapshot{} = snapshot) do
    case Map.get(snapshot.overwrites, {:role, snapshot.everyone_role_id}) do
      nil -> false
      ow -> Permission.has?(ow.deny, :view_channel)
    end
  end

  # ---- Management hierarchy (section 3.3) ------------------------------------

  @doc """
  Whether the actor may edit, assign, or delete a role: MANAGE_ROLES at the
  server level and the role is positioned below the actor's highest, or the
  actor is owner. Lower position sorts first and is higher.
  """
  def can_manage_role?(%{owner?: true}, _role), do: true

  def can_manage_role?(actor, role) do
    manages? =
      Permission.has?(actor.server_permissions, :manage_roles) or
        Permission.has?(actor.server_permissions, :administrator)

    manages? and above?(actor, role.position)
  end

  @doc "Whether the actor may grant every permission in `bits`."
  def can_grant?(%{owner?: true}, _bits), do: true

  def can_grant?(actor, bits) do
    Permission.has?(actor.server_permissions, :administrator) or
      Permission.includes?(actor.server_permissions, bits)
  end

  @doc """
  Whether the actor may kick or ban a target: MANAGE_MEMBERS, the target
  is not the owner, and the target's highest role is below the actor's.
  """
  def can_remove_member?(_actor, %{owner?: true}), do: false
  def can_remove_member?(%{owner?: true}, _target), do: true

  def can_remove_member?(actor, target) do
    manages? =
      Permission.has?(actor.server_permissions, :manage_members) or
        Permission.has?(actor.server_permissions, :administrator)

    manages? and above?(actor, target.highest_position)
  end

  # An actor with no roles has no position and is above nothing. A target
  # with no roles (nil) is below everyone who has one.
  defp above?(%{highest_position: nil}, _), do: false
  defp above?(_actor, nil), do: true
  defp above?(%{highest_position: mine}, theirs), do: mine < theirs
end
