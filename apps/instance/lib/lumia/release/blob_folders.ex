defmodule Lumia.Release.BlobFolders do
  @moduledoc """
  One-off (2026-11-15): moves the instance's pictures into a folder per
  kind in the blob store (`avatars/`, `icons/`, `cards/`), as
  `Lumia.Blobs` names them now, from the store's top level where they were
  named before. Run once per database by the migration
  `20261116000000_move_blobs_into_folders`. Delete this module and that
  migration once every instance that matters (the flagship) has run it.

  * An account's picture or a card's moves to its folder under the same
    address.
  * A server's picture was kept as an `"avatar"`; it becomes an `"icon"`,
    whose address is under the icon key, so it gets a new address (its
    content key wrapped again for it) and the server's `icon_url` follows.
  * A directory store's attachments move from the old layout
    (`<dir>/at/att/<id>`) to the new (`<dir>/att/<two>/<id>`).

  Every step can run again: an object already moved is left, one not
  there is skipped and logged.
  """

  import Ecto.Query

  alias Lumia.Blobs
  alias Lumia.Blobs.Blob
  alias Lumia.Blobs.Store
  alias Lumia.Repo
  alias Lumia.Servers.Server

  require Logger

  @avatar_path "/api/v1/avatars/"

  @doc "Moves everything; returns how many objects moved."
  def run do
    case Store.configured() do
      nil ->
        0

      store ->
        if match?({:s3, _}, store), do: {:ok, _} = Application.ensure_all_started(:req)
        moved = move_pictures() + move_local_attachments(Store.directory())
        Logger.info("blob folders: #{moved} objects moved")
        moved
    end
  end

  defp move_pictures do
    icons =
      Repo.all(from s in Server, where: not is_nil(s.icon_url), select: s.icon_url)
      |> MapSet.new()

    Repo.all(Blob)
    |> Enum.map(fn blob ->
      if blob.kind == "avatar" and MapSet.member?(icons, @avatar_path <> Blobs.name(blob.address)),
        do: to_icon(blob),
        else: move(Blobs.name(blob.address), Blobs.object_name(blob.kind, blob.address))
    end)
    |> Enum.count(& &1)
  end

  # `from` to `to` in the store; true when something moved.
  defp move(from, to) do
    case Store.get(from) do
      {:ok, object} ->
        :ok = Store.put(to, object)
        :ok = Store.delete(from)
        true

      {:error, :not_found} ->
        if not Store.exists?(to), do: Logger.warning("blob folders: #{from} is in neither place")
        false
    end
  end

  defp to_icon(%Blob{} = blob) do
    old_path = @avatar_path <> Blobs.name(blob.address)
    flat = Blobs.name(blob.address)

    with {:ok, object} <- Store.get(flat),
         new_address = Blobs.address_for("icon", object),
         {:ok, wrapped} <- Blobs.rewrap(blob, new_address),
         :ok <- Store.put(Blobs.object_name("icon", new_address), object) do
      {:ok, _} =
        Lumia.Tx.transact(fn ->
          Repo.insert!(%{blob | address: new_address, kind: "icon", wrapped_key: wrapped},
            on_conflict: :nothing
          )

          Repo.update_all(from(s in Server, where: s.icon_url == ^old_path),
            set: [icon_url: @avatar_path <> Blobs.name(new_address)]
          )

          Repo.delete_all(from b in Blob, where: b.address == ^blob.address)
          {:ok, nil}
        end)

      :ok = Store.delete(flat)
      true
    else
      error ->
        Logger.warning("blob folders: server picture #{flat} not moved: #{inspect(error)}")
        false
    end
  end

  defp move_local_attachments(nil), do: 0

  defp move_local_attachments(dir) do
    for old <- Path.wildcard(Path.join([dir, "at", "att", "*"])),
        id = Path.basename(old),
        new = Store.Local.path(dir, "att/" <> id),
        :ok <- [File.mkdir_p(Path.dirname(new))],
        :ok <- [File.rename(old, new)],
        reduce: 0 do
      n -> n + 1
    end
  end
end
