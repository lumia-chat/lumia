defmodule Lumia.AuthzReference do
  @moduledoc """
  The reference implementation of docs/specs/authorization.md section 3.1,
  written over `MapSet`s of permission names with no regard for speed. The
  bitfield implementation in `Lumia.Authz` is property-tested against it.
  """

  alias Lumia.Authz.Permission

  @all MapSet.new(Permission.names())

  def all, do: @all

  def resolve(_snapshot, %{owner?: true}), do: all()

  def resolve(snapshot, member) do
    base = base(snapshot, member)

    if MapSet.member?(base, :administrator) do
      all()
    else
      base
      |> step4(snapshot, member)
      |> step5(snapshot, member)
      |> step6(snapshot, member)
    end
  end

  defp base(_snapshot, %{kind: :guest}), do: MapSet.new()

  defp base(snapshot, %{role_ids: role_ids}) do
    [snapshot.everyone_role_id | role_ids]
    |> Enum.map(&role_set(snapshot, &1))
    |> Enum.reduce(MapSet.new(), &MapSet.union/2)
  end

  defp role_set(snapshot, role_id) do
    case Map.get(snapshot.roles, role_id) do
      nil -> MapSet.new()
      role -> MapSet.new(Permission.from_bits(role.permissions))
    end
  end

  defp step4(set, _snapshot, %{kind: :guest}), do: set

  defp step4(set, snapshot, _member) do
    apply_ow(set, Map.get(snapshot.overwrites, {:role, snapshot.everyone_role_id}))
  end

  defp step5(set, snapshot, %{role_ids: role_ids}) do
    ows =
      role_ids |> Enum.map(&Map.get(snapshot.overwrites, {:role, &1})) |> Enum.reject(&is_nil/1)

    deny =
      ows
      |> Enum.map(&MapSet.new(Permission.from_bits(&1.deny)))
      |> Enum.reduce(MapSet.new(), &MapSet.union/2)

    allow =
      ows
      |> Enum.map(&MapSet.new(Permission.from_bits(&1.allow)))
      |> Enum.reduce(MapSet.new(), &MapSet.union/2)

    set |> MapSet.difference(deny) |> MapSet.union(allow)
  end

  defp step6(set, snapshot, %{account_id: account_id}) do
    apply_ow(set, Map.get(snapshot.overwrites, {:member, account_id}))
  end

  defp apply_ow(set, nil), do: set

  defp apply_ow(set, %{allow: allow, deny: deny}) do
    set
    |> MapSet.difference(MapSet.new(Permission.from_bits(deny)))
    |> MapSet.union(MapSet.new(Permission.from_bits(allow)))
  end
end
