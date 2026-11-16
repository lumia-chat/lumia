defmodule Lumia.Blobs.Store.Local do
  @moduledoc """
  Blobs as files under a directory: a name `folder/rest` (`avatars/<hex>`,
  `att/<uuid>`) at `<dir>/<folder>/<first two of rest>/<rest>`.
  A write goes to a temporary file beside its place and is renamed in, so a
  reader never sees half of one.
  """

  def put(dir, name, bytes) do
    path = path(dir, name)
    partial = path <> ".part-" <> Integer.to_string(System.unique_integer([:positive]))

    with :ok <- File.mkdir_p(Path.dirname(path)),
         :ok <- File.write(partial, bytes),
         :ok <- File.rename(partial, path) do
      :ok
    else
      {:error, reason} ->
        File.rm(partial)
        {:error, reason}
    end
  end

  def get(dir, name) do
    case File.read(path(dir, name)) do
      {:ok, bytes} -> {:ok, bytes}
      {:error, :enoent} -> {:error, :not_found}
      {:error, reason} -> {:error, reason}
    end
  end

  def delete(dir, name) do
    case File.rm(path(dir, name)) do
      :ok -> :ok
      {:error, :enoent} -> :ok
      {:error, reason} -> {:error, reason}
    end
  end

  def exists?(dir, name), do: File.regular?(path(dir, name))

  def size(dir, name) do
    case File.stat(path(dir, name)) do
      {:ok, %File.Stat{type: :regular, size: size}} -> {:ok, size}
      _ -> {:error, :not_found}
    end
  end

  @doc "Where `name` is kept under `dir`."
  def path(dir, name) do
    # Names are made by `Lumia.Blobs` and `Lumia.Uploads`, never a caller's string.
    case String.split(name, "/", parts: 2) do
      [folder, <<prefix::binary-size(2), _::binary>> = rest] ->
        Path.join([dir, folder, prefix, rest])

      # A name with no folder, as blobs were named before they had one
      # (`Lumia.Release.BlobFolders` moves them).
      [<<prefix::binary-size(2), _::binary>>] ->
        Path.join([dir, prefix, name])
    end
  end
end
