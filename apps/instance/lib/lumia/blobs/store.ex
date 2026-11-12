defmodule Lumia.Blobs.Store do
  @moduledoc """
  The blob store (docs/specs/deployment.md, `LUMIA_BLOB_STORE`): where the
  ciphertext of a blob is kept under its name. One of

    * `{:local, path}`: a directory on this node's disk, the single-node
      default; refused at boot when clustering is on, since a blob
      written on one node would be missing on the next;
    * `{:fs, path}`: the same, on a filesystem every node mounts;
    * `{:s3, options}`: an S3-compatible bucket.

  A store sees only names and ciphertext (`Lumia.Blobs` encrypts first).
  """

  alias Lumia.Blobs.Store.Local
  alias Lumia.Blobs.Store.S3

  @doc "The configured store, or nil when the instance keeps no blobs."
  def configured, do: Application.get_env(:lumia, :blob_store)

  @doc "Writes `bytes` under `name`, replacing what was there."
  def put(name, bytes), do: call(:put, [name, bytes])

  @doc "`{:ok, bytes}`, or `{:error, :not_found}`."
  def get(name), do: call(:get, [name])

  @doc "Removes `name`; fine when it is not there."
  def delete(name), do: call(:delete, [name])

  @doc "Whether `name` is in the store."
  def exists?(name), do: call(:exists?, [name]) == true

  @doc "The store's kind as a backup's manifest names it: `local`, `fs`, `s3` or `none`."
  def kind do
    case configured() do
      {kind, _} -> Atom.to_string(kind)
      nil -> "none"
    end
  end

  @doc "The directory of a `local` or `fs` store, which a backup copies; nil for any other."
  def directory do
    case configured() do
      {kind, path} when kind in [:local, :fs] -> path
      _ -> nil
    end
  end

  defp call(function, args) do
    case configured() do
      {kind, path} when kind in [:local, :fs] -> apply(Local, function, [path | args])
      {:s3, options} -> apply(S3, function, [options | args])
      nil -> {:error, :unavailable}
    end
  end
end
