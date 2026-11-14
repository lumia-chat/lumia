defmodule Lumia.Rails do
  @moduledoc """
  The server rail as an account arranged it (architecture 6.9): its servers
  in an order of its own, some gathered in folders, one arrangement per
  account that every device follows.

  What is kept is always tidied against the account's memberships: a
  server it is not in is dropped, a server named twice is kept at its first
  place, a folder left empty goes, and a server it is in that the
  arrangement leaves out goes at the end, in the order it was joined. So a
  server joined or created lands last, with no write, since the arrangement
  is tidied as it is read. A server left is also taken out of what is kept
  (`forget/2`, as the membership ends), so one joined again lands last too,
  not back in its old place or folder.
  """

  import Ecto.Query

  alias Lumia.Accounts
  alias Lumia.Proto
  alias Lumia.Rails.AccountRail
  alias Lumia.Repo
  alias Lumia.Servers.Member
  alias Lumia.Tx
  alias Lumia.V1

  @tones ~w(peach blue purple mint amber sky stone green indigo)
  @max_folders 100
  @max_name 50

  @doc "The nine tones a folder may have."
  def tones, do: @tones

  @doc """
  The account's rail, tidied against the servers it is in now: `joined`,
  their ids in the order it joined them, when the caller has read them
  already (the snapshot has), else read here.
  """
  def rail(account_id, joined \\ nil) do
    saved =
      case Repo.get(AccountRail, account_id) do
        %AccountRail{rail: bytes} -> V1.Rail.decode(bytes)
        nil -> %V1.Rail{}
      end

    tidy(saved, joined || joined(account_id))
  end

  @doc "The order `joined/1` reads the account's memberships in, for a caller reading them itself."
  def joined_order(query), do: order_by(query, [m], asc: m.joined_at, asc: m.server_id)

  @doc """
  Keeps `rail` as the account's arrangement, tidied, and tells its devices
  (`RailArranged`). The latest write wins. `{:ok, rail}` with what is kept,
  or `{:error, {:invalid_argument, field}}`.
  """
  def arrange(account_id, %V1.Rail{} = rail) do
    with :ok <- check(rail), do: Tx.transact(fn -> keep(account_id, rail) end)
  end

  defp keep(account_id, rail) do
    kept = tidy(rail, joined(account_id))
    bytes = V1.Rail.encode(kept)
    now = DateTime.utc_now()

    Repo.insert!(%AccountRail{account_id: account_id, rail: bytes, updated_at: now},
      on_conflict: [set: [rail: bytes, updated_at: now]],
      conflict_target: :account_id
    )

    with {:ok, _} <-
           Accounts.append_event(account_id, {:rail_arranged, %V1.RailArranged{rail: kept}}),
         do: {:ok, kept}
  end

  @doc """
  Takes a server the account has left out of its kept arrangement, a folder
  left empty with it. Nobody is told: every read already leaves it out.
  """
  def forget(account_id, server_id) do
    with %AccountRail{rail: bytes} = kept <- Repo.get(AccountRail, account_id) do
      gone = Proto.uuid(server_id)
      %V1.Rail{entries: entries} = V1.Rail.decode(bytes)

      rail = %V1.Rail{
        entries: Enum.flat_map(entries, &without(&1, gone))
      }

      kept
      |> Ecto.Changeset.change(rail: V1.Rail.encode(rail), updated_at: DateTime.utc_now())
      |> Repo.update!()
    end

    :ok
  end

  defp without(%V1.RailEntry{entry: {:server_id, gone}}, gone), do: []

  defp without(%V1.RailEntry{entry: {:folder, %V1.RailFolder{} = folder}} = entry, gone) do
    case Enum.reject(folder.server_ids, &(&1 == gone)) do
      [] -> []
      inside -> [%{entry | entry: {:folder, %{folder | server_ids: inside}}}]
    end
  end

  defp without(entry, _gone), do: [entry]

  # The servers the account is in, in the order it joined them.
  defp joined(account_id) do
    from(m in Member, where: m.account_id == ^account_id, select: m.server_id)
    |> joined_order()
    |> Repo.all()
  end

  @doc false
  # Every server in `servers` placed once: the arrangement's where it names
  # them, the rest after, in `servers`' order. Exposed for its test.
  def tidy(%V1.Rail{entries: entries}, servers) do
    member = MapSet.new(servers)
    {kept, placed} = Enum.reduce(entries, {[], MapSet.new()}, &keep_entry(&1, &2, member))

    rest =
      for id <- servers,
          !MapSet.member?(placed, id),
          do: %V1.RailEntry{entry: {:server_id, Proto.uuid(id)}}

    %V1.Rail{entries: Enum.reverse(kept) ++ rest}
  end

  defp keep_entry(%V1.RailEntry{entry: {:server_id, bytes}} = entry, {kept, placed}, member) do
    case place(bytes, placed, member) do
      {:ok, _id, placed} -> {[entry | kept], placed}
      :skip -> {kept, placed}
    end
  end

  defp keep_entry(
         %V1.RailEntry{entry: {:folder, %V1.RailFolder{} = folder}},
         {kept, placed},
         member
       ) do
    {inside, placed} = Enum.reduce(folder.server_ids, {[], placed}, &keep_inside(&1, &2, member))

    case inside do
      [] ->
        {kept, placed}

      _ ->
        folder = %{folder | server_ids: Enum.reverse(inside), name: String.trim(folder.name)}
        {[%V1.RailEntry{entry: {:folder, folder}} | kept], placed}
    end
  end

  defp keep_entry(_entry, acc, _member), do: acc

  defp keep_inside(bytes, {inside, placed}, member) do
    case place(bytes, placed, member) do
      {:ok, id, placed} -> {[Proto.uuid(id) | inside], placed}
      :skip -> {inside, placed}
    end
  end

  # A server goes where it is first named, if the account is in it.
  defp place(bytes, placed, member) do
    id = Proto.from_uuid(bytes)

    if id && MapSet.member?(member, id) && !MapSet.member?(placed, id),
      do: {:ok, id, MapSet.put(placed, id)},
      else: :skip
  end

  # What no tidying makes right is refused.
  defp check(%V1.Rail{entries: entries}) do
    folders = for %V1.RailEntry{entry: {:folder, folder}} <- entries, do: folder
    ids = Enum.map(folders, & &1.folder_id)

    cond do
      length(folders) > @max_folders ->
        {:error, {:invalid_argument, "folders"}}

      Enum.any?(ids, &(byte_size(&1) != 16)) ->
        {:error, {:invalid_argument, "folder_id"}}

      length(Enum.uniq(ids)) != length(ids) ->
        {:error, {:invalid_argument, "folder_id"}}

      Enum.any?(folders, &(String.length(String.trim(&1.name)) > @max_name)) ->
        {:error, {:invalid_argument, "name"}}

      Enum.any?(folders, &(&1.tone not in @tones)) ->
        {:error, {:invalid_argument, "tone"}}

      true ->
        :ok
    end
  end
end
