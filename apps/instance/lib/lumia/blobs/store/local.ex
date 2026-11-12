defmodule Lumia.Blobs.Store.Local do
  @moduledoc """
  Blobs as files under a directory: `<dir>/<first two of the name>/<name>`.
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

  # Names are hex made by `Lumia.Blobs`, never a caller's string.
  defp path(dir, <<prefix::binary-size(2), _::binary>> = name), do: Path.join([dir, prefix, name])
end
