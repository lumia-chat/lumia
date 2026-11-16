defmodule Lumia.Repo.Migrations.MoveBlobsIntoFolders do
  @moduledoc """
  One-off (2026-11-15): the instance's pictures into a folder per kind in
  the blob store, server pictures as their own kind
  (`Lumia.Release.BlobFolders`). Data only, run once per database on
  deploy. Delete this file with that module once the flagship has run it.
  """

  use Ecto.Migration

  def up, do: Lumia.Release.BlobFolders.run()

  def down, do: :ok
end
