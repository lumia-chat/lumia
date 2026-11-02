defmodule Lumia.Release do
  @moduledoc """
  The release commands of docs/specs/deployment.md sections 4 and 6, run
  by `bin/lumia-admin` through `eval`: the application is loaded, never
  started, and each command starts the repo for its own duration.

  A backup is `database.dump` (`pg_dump`, custom format, one serializable
  snapshot) and `manifest.json`: the schema version, the recovery
  generation, the host, the time, the blob store kind, and the SHA-256
  fingerprint of the master key, never the key. Blobs are listed as
  `none` until attachments exist.

  A restore refuses a manifest whose master-key fingerprint differs from
  the configured key's, or whose schema is newer than this release knows,
  or a database that is not empty unless told `replace: true`. It
  restores, migrates up, and bumps the recovery generation so every
  client takes the snapshot path on its next connect.
  """

  alias Lumia.Instance
  alias Lumia.Repo

  @app :lumia

  @doc "Runs every pending migration."
  def migrate do
    load()

    for repo <- repos() do
      {:ok, _, _} = Ecto.Migrator.with_repo(repo, &Ecto.Migrator.run(&1, :up, all: true))
    end

    :ok
  end

  @doc "Rolls one repo back to `version`."
  def rollback(repo, version) do
    load()
    {:ok, _, _} = Ecto.Migrator.with_repo(repo, &Ecto.Migrator.run(&1, :down, to: version))
    :ok
  end

  @doc """
  Writes a backup set into `dir`. Options: `:url` for the database when
  not the configured repo's (tests), `:pg_bin` for the Postgres client
  tools' directory when not on the path.
  """
  def backup(dir, opts \\ []) do
    load()
    File.mkdir_p!(dir)
    url = opts[:url] || database_url()

    with_repo(url, fn _ ->
      run!(opts, "pg_dump", [
        "--format=custom",
        "--serializable-deferrable",
        "--no-owner",
        "--file",
        Path.join(dir, "database.dump"),
        url
      ])

      manifest = %{
        "manifest_version" => 1,
        "schema_version" => schema_version(),
        "recovery_generation" => Instance.reload().recovery_generation,
        "host" => Lumia.Identity.host(),
        "taken_at" => DateTime.to_iso8601(DateTime.utc_now()),
        "blob_store" => "none",
        "master_key_fingerprint" => master_key_fingerprint()
      }

      File.write!(Path.join(dir, "manifest.json"), Jason.encode!(manifest, pretty: true))
      {:ok, manifest}
    end)
  end

  @doc """
  Restores the set in `dir`. Options: `:url` for the target database
  (tests), `:replace` to overwrite a database that is not empty,
  `:pg_bin` as for `backup/2`. Returns the new recovery generation.
  """
  def restore(dir, opts \\ []) do
    load()
    manifest = dir |> Path.join("manifest.json") |> File.read!() |> Jason.decode!()
    url = opts[:url] || database_url()

    with :ok <- check_fingerprint(manifest["master_key_fingerprint"]),
         :ok <- check_schema(manifest["schema_version"]),
         :ok <- check_empty(url, opts[:replace] == true) do
      # `--replace` empties the schema first; pg_restore's own `--clean`
      # cannot drop the inherited constraints of the partitioned logs.
      if opts[:replace] == true, do: reset_schema(url)

      run!(opts, "pg_restore", [
        "--no-owner",
        "--exit-on-error",
        "--dbname",
        url,
        Path.join(dir, "database.dump")
      ])

      with_repo(url, fn pid ->
        Ecto.Migrator.run(Repo, :up, all: true, dynamic_repo: pid)
        Instance.reload()
        generation = Instance.bump_recovery_generation()
        {:ok, generation}
      end)
    end
  end

  # ---- Checks ----------------------------------------------------------------------

  defp check_fingerprint(nil), do: :ok

  defp check_fingerprint(expected) do
    case master_key_fingerprint() do
      nil -> :ok
      ^expected -> :ok
      _ -> {:error, {:master_key_mismatch, expected}}
    end
  end

  defp check_schema(nil), do: :ok

  defp check_schema(version) do
    if version <= schema_version(),
      do: :ok,
      else: {:error, {:schema_newer_than_release, version, schema_version()}}
  end

  defp check_empty(_url, true), do: :ok

  defp check_empty(url, false) do
    with_repo(url, fn _ ->
      %{rows: [[count]]} =
        Repo.query!(
          "select count(*) from pg_tables where schemaname = 'public' and tablename <> 'schema_migrations'"
        )

      if count == 0, do: :ok, else: {:error, {:database_not_empty, count}}
    end)
  end

  defp reset_schema(url) do
    with_repo(url, fn _ ->
      Repo.query!("drop schema public cascade")
      Repo.query!("create schema public")
      :ok
    end)
  end

  # ---- Helpers ---------------------------------------------------------------------

  # A release's `eval` loads nothing: the application's code and the
  # database stack have to be brought up by hand (never the application
  # itself, which would start the server).
  defp load do
    :ok = Application.ensure_loaded(@app)
    {:ok, _} = Application.ensure_all_started([:ssl, :ecto_sql, :postgrex])
    :ok
  end

  defp repos, do: Application.fetch_env!(@app, :ecto_repos)

  # A repo of its own on `url`, routed to by `Repo` calls in this process
  # while `fun` runs: independent of whether the application's repo is up
  # (a release's `eval`) or sandboxed (the tests), and of which database it
  # points at.
  defp with_repo(url, fun) do
    config =
      Application.fetch_env!(@app, Repo)
      |> Keyword.drop([:username, :password, :hostname, :database, :port, :pool])
      |> Keyword.merge(name: nil, url: url, pool_size: 2)

    {:ok, pid} = Repo.start_link(config)
    previous = Repo.get_dynamic_repo()
    Repo.put_dynamic_repo(pid)

    try do
      fun.(pid)
    after
      Repo.put_dynamic_repo(previous)
      Supervisor.stop(pid)
    end
  end

  @doc false
  def schema_version do
    case Path.wildcard(Path.join([:code.priv_dir(@app), "repo", "migrations", "*.exs"])) do
      [] ->
        0

      files ->
        files
        |> Enum.map(&(&1 |> Path.basename() |> String.split("_") |> hd() |> String.to_integer()))
        |> Enum.max()
    end
  end

  @doc false
  def master_key_fingerprint do
    case Application.get_env(@app, :master_key_file) do
      nil -> nil
      path -> :crypto.hash(:sha256, File.read!(path)) |> Base.encode16(case: :lower)
    end
  end

  @doc false
  def database_url do
    config = Application.fetch_env!(@app, Repo)

    config[:url] ||
      %URI{
        scheme: "postgres",
        userinfo: "#{config[:username]}:#{config[:password]}",
        host: config[:hostname] || "localhost",
        port: config[:port] || 5432,
        path: "/#{config[:database]}"
      }
      |> URI.to_string()
  end

  defp run!(opts, tool, args) do
    exe =
      case opts[:pg_bin] || System.get_env("PG_BIN") do
        nil -> System.find_executable(tool) || raise "#{tool} is not on the path; set PG_BIN"
        dir -> Path.join(dir, tool)
      end

    case System.cmd(exe, args, stderr_to_stdout: true) do
      {_, 0} -> :ok
      {out, code} -> raise "#{tool} failed (#{code}): #{out}"
    end
  end
end
