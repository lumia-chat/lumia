defmodule Lumia.Release.BlobFoldersTest do
  @moduledoc """
  The one-off move of the instance's pictures into folders
  (`Lumia.Release.BlobFolders`): from the old layout, an account's picture
  and a card's keep their addresses, a server's becomes an icon at a new
  one with the server following, a directory store's attachments move to
  the new layout, and running it again moves nothing.
  """

  use Lumia.DataCase, async: false

  import Lumia.Fixtures

  alias Lumia.Accounts
  alias Lumia.Blobs
  alias Lumia.Blobs.Blob
  alias Lumia.Blobs.Store
  alias Lumia.Release.BlobFolders
  alias Lumia.Repo
  alias Lumia.Servers
  alias Lumia.Servers.Server

  # Puts a blob back where the old code kept it: at the top, by its address.
  defp unfolder(%Blob{} = blob, kind_was) do
    new = Blobs.object_name(kind_was, blob.address)
    {:ok, object} = Store.get(new)
    :ok = Store.put(Blobs.name(blob.address), object)
    :ok = Store.delete(new)
  end

  test "pictures move into their folders, a server's becoming an icon" do
    owner = account("owner")
    {:ok, server} = Servers.create_server(owner.id, "Acme")

    {:ok, avatar} = Blobs.put("an account's picture", "avatar", "image/png", owner.id)
    {:ok, card} = Blobs.put("a card's picture", "card", "image/jpeg", nil)

    # A server's picture as the old code kept it: an "avatar".
    {:ok, icon} = Blobs.put("a server's picture", "avatar", "image/png", owner.id)
    icon_path = Accounts.picture_path(icon)
    Repo.update_all(from(s in Server, where: s.id == ^server.id), set: [icon_url: icon_path])

    unfolder(avatar, "avatar")
    unfolder(card, "card")
    unfolder(icon, "avatar")

    # A directory store's attachment in the old layout.
    dir = Store.directory()
    id = Ecto.UUID.generate()
    old_attachment = Path.join([dir, "at", "att", id])
    File.mkdir_p!(Path.dirname(old_attachment))
    File.write!(old_attachment, "ciphertext")

    assert BlobFolders.run() == 4

    assert {:ok, %{bytes: "an account's picture"}} = Blobs.fetch(avatar.address, "avatar")
    assert {:ok, %{bytes: "a card's picture"}} = Blobs.fetch(card.address, "card")
    assert Store.exists?("avatars/" <> Blobs.name(avatar.address))
    refute Store.exists?(Blobs.name(avatar.address))

    moved = Repo.get!(Server, server.id)
    assert moved.icon_url != icon_path
    "/api/v1/avatars/" <> name = moved.icon_url
    address = Blobs.address(name)
    assert {:ok, %{bytes: "a server's picture"}} = Blobs.fetch(address, ["avatar", "icon"])
    assert %Blob{kind: "icon"} = Repo.get(Blob, address)
    refute Repo.get(Blob, icon.address)
    refute Store.exists?(Blobs.name(icon.address))

    assert Store.get("att/" <> id) == {:ok, "ciphertext"}
    refute File.exists?(old_attachment)

    assert BlobFolders.run() == 0
  end
end
