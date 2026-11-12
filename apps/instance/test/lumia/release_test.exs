defmodule Lumia.ReleaseTest do
  @moduledoc """
  The backup and restore commands (docs/specs/deployment.md section 4),
  against the test database for the backup and a scratch database for
  the restore. Needs the Postgres client tools of the server's major on
  the path (or `PG_BIN`).
  """

  use ExUnit.Case, async: false

  alias Ecto.Adapters.Postgres
  alias Ecto.Adapters.SQL.Sandbox
  alias Lumia.Blobs
  alias Lumia.Instance
  alias Lumia.Release

  @scratch "lumia_release_test#{System.get_env("MIX_TEST_PARTITION")}"

  setup_all do
    # CI configures the repo by URL; the storage helpers want the fields.
    config = Application.fetch_env!(:lumia, Lumia.Repo)

    config =
      case Keyword.pop(config, :url) do
        {nil, config} -> config
        {url, config} -> Keyword.merge(config, Ecto.Repo.Supervisor.parse_url(url))
      end
      |> Keyword.put(:database, @scratch)

    _ = Postgres.storage_down(config)
    :ok = Postgres.storage_up(config)

    on_exit(fn ->
      _ = Postgres.storage_down(config)
      Application.delete_env(:lumia, :master_key_file)
      # The restore loaded the scratch database's settings into persistent_term.
      Sandbox.checkout(Lumia.Repo)
      Instance.reload()
    end)

    url = Release.database_url() |> String.replace(~r{/[^/]+$}, "/#{@scratch}")

    %{
      dir: Path.join(System.tmp_dir!(), "lumia-backup-#{System.unique_integer([:positive])}"),
      scratch_url: url
    }
  end

  test "backup writes the set, restore refuses a wrong key and otherwise bumps the generation",
       ctx do
    key = Path.join(System.tmp_dir!(), "lumia-key-#{System.unique_integer([:positive])}")
    File.write!(key, :crypto.strong_rand_bytes(32))
    Application.put_env(:lumia, :master_key_file, key)

    # A picture, so the set has a blob to carry. Committed for real: the
    # dump is another connection's view of the database.
    Sandbox.checkout(Lumia.Repo, sandbox: false)

    {:ok, owner} =
      Lumia.Accounts.create_account(%{handle: "backup#{System.unique_integer([:positive])}"})

    {:ok, blob} = Blobs.put("a picture's bytes", "avatar", "image/png", owner.id)
    name = Blobs.name(blob.address)
    stored = Path.join([Blobs.Store.directory(), String.slice(name, 0, 2), name])

    on_exit(fn ->
      Sandbox.checkout(Lumia.Repo, sandbox: false)
      Blobs.delete(blob.address)
      Lumia.Repo.delete(owner)
    end)

    assert {:ok, manifest} = Release.backup(ctx.dir)
    assert File.regular?(Path.join(ctx.dir, "database.dump"))
    assert manifest["blob_store"] == "local"
    assert File.regular?(Path.join([ctx.dir, "blobs", String.slice(name, 0, 2), name]))
    assert manifest["schema_version"] == Release.schema_version()
    assert manifest["master_key_fingerprint"] == Release.master_key_fingerprint()
    assert is_integer(manifest["recovery_generation"])

    # A different key: refused before anything is written.
    File.write!(key, :crypto.strong_rand_bytes(32))
    assert {:error, {:master_key_mismatch, _}} = Release.restore(ctx.dir, url: ctx.scratch_url)

    manifest_path = Path.join(ctx.dir, "manifest.json")

    File.write!(
      manifest_path,
      Jason.encode!(Map.put(manifest, "master_key_fingerprint", Release.master_key_fingerprint()))
    )

    # The store lost the blob since: the restore puts it back from the set.
    File.rm!(stored)
    assert {:ok, generation} = Release.restore(ctx.dir, url: ctx.scratch_url)
    assert generation == manifest["recovery_generation"] + 1
    assert File.regular?(stored)

    # A set without it, over a store without it: refused by name, unless told to go on.
    File.rm!(stored)
    File.rm_rf!(Path.join(ctx.dir, "blobs"))

    assert {:error, {:missing_blobs, [^name]}} =
             Release.restore(ctx.dir, url: ctx.scratch_url, replace: true)

    assert {:ok, _} =
             Release.restore(ctx.dir,
               url: ctx.scratch_url,
               replace: true,
               allow_missing_blobs: true
             )

    # The scratch database holds the restored settings; a second restore needs --replace.
    assert {:error, {:database_not_empty, _}} = Release.restore(ctx.dir, url: ctx.scratch_url)

    assert {:ok, again} =
             Release.restore(ctx.dir,
               url: ctx.scratch_url,
               replace: true,
               allow_missing_blobs: true
             )

    assert again == manifest["recovery_generation"] + 1

    File.rm_rf!(ctx.dir)
    File.rm!(key)
  end
end
