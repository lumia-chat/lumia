defmodule Lumia.PeerNode do
  @moduledoc """
  A second BEAM node running this application against the same database,
  for the cross-node cases (docs/specs/deployment.md section 8). Started
  with OTP's `:peer`, so its control channel survives a partition made
  with `Node.disconnect/1`; `call/4` goes over that channel.

  The test node itself becomes distributed on first use (`epmd` is started
  if needed). The peer gets this node's application environment, with the
  repo on a plain connection pool rather than the sandbox: rows the peer
  writes are real, so a test using it runs with the sandbox in `:auto`
  mode and cleans up after itself (`truncate_all/0`).
  """

  @cookie :lumia_test

  @doc "Starts a peer named `name` and boots `:lumia` on it. Returns `{pid, node}`."
  def start(name) when is_atom(name) do
    ensure_distributed()
    paths = Enum.map(:code.get_path(), &to_charlist/1)

    {:ok, pid, node} =
      :peer.start_link(%{
        name: name,
        host: ~c"127.0.0.1",
        # A TCP control channel on a free port, so calls work before the
        # nodes connect and across a partition.
        connection: 0,
        args: [~c"-setcookie", Atom.to_charlist(@cookie), ~c"-pa" | paths]
      })

    # This node's configuration, for every application but the runtime's own.
    for {app, _, _} <- Application.loaded_applications(),
        app not in [:kernel, :stdlib, :sasl, :elixir, :mix, :ex_unit, :iex, :compiler],
        {key, value} <- Application.get_all_env(app) do
      # Persistent, or loading the application on the peer would replace it.
      :ok = :peer.call(pid, Application, :put_env, [app, key, value, [persistent: true]])
    end

    repo =
      Application.get_env(:lumia, Lumia.Repo)
      |> Keyword.put(:pool, DBConnection.ConnectionPool)
      |> Keyword.put(:pool_size, 2)

    :ok = :peer.call(pid, Application, :put_env, [:lumia, Lumia.Repo, repo, [persistent: true]])
    :ok = :peer.call(pid, Logger, :configure, [[level: :warning]])
    {:ok, _} = :peer.call(pid, Application, :ensure_all_started, [:lumia], 60_000)
    true = Node.connect(node)
    {pid, node}
  end

  @doc "Calls `m.f(args)` on the peer over its control channel, partition or not."
  def call(pid, m, f, args), do: :peer.call(pid, m, f, args)

  @doc "Stops the peer."
  def stop(pid), do: :peer.stop(pid)

  @doc "Empties every table but the migrations and the instance settings, after a non-sandboxed test."
  def truncate_all do
    tables =
      Lumia.Repo.query!(
        "select tablename from pg_tables where schemaname = 'public' and tablename not in ('schema_migrations', 'instance_settings')"
      ).rows
      |> List.flatten()

    Lumia.Repo.query!("truncate table #{Enum.map_join(tables, ", ", &~s("#{&1}"))} cascade")
    :ok
  end

  defp ensure_distributed do
    if Node.alive?() do
      :ok
    else
      {_, 0} = System.cmd("epmd", ["-daemon"])
      {:ok, _} = Node.start(:"lumia_test@127.0.0.1")
      Node.set_cookie(@cookie)
      :ok
    end
  end
end
