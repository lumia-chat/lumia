defmodule Lumia.Authz.Cache do
  @moduledoc """
  Per-node cache of ACL snapshots keyed `(channel_id, acl_version)`
  (docs/specs/authorization.md section 4).

  Because the version is in the key, an entry is immutable once loaded and
  needs no invalidation; a change to the ACL bumps the version and the old
  entry simply stops being asked for. A sweeper evicts entries older than
  the TTL. The GenServer owns the table; reads go straight to ETS.
  """

  use GenServer

  alias Lumia.Authz.Snapshot

  @table __MODULE__
  @ttl_ms :timer.minutes(10)
  @sweep_ms :timer.minutes(1)

  def start_link(opts), do: GenServer.start_link(__MODULE__, opts, name: __MODULE__)

  @doc """
  The snapshot for a channel at a version. A miss loads from Postgres and
  verifies the loaded version matches; a stale request (the channel has
  moved on) returns `{:error, :stale}` so the caller re-reads the version.
  """
  @spec fetch(Ecto.UUID.t(), pos_integer()) ::
          {:ok, Snapshot.t()} | {:error, :stale | :channel_not_found}
  def fetch(channel_id, acl_version) do
    case :ets.lookup(@table, {channel_id, acl_version}) do
      [{_, snapshot, _}] -> {:ok, snapshot}
      [] -> load_version(channel_id, acl_version)
    end
  end

  defp load_version(channel_id, acl_version) do
    with {:ok, snapshot} <- Snapshot.load(channel_id) do
      if snapshot.acl_version == acl_version do
        :ets.insert(@table, {{channel_id, acl_version}, snapshot, now()})
        {:ok, snapshot}
      else
        {:error, :stale}
      end
    end
  end

  @doc "The snapshot at the channel's current version, loading if needed."
  @spec fetch_current(Ecto.UUID.t()) :: {:ok, Snapshot.t()} | {:error, :channel_not_found}
  def fetch_current(channel_id) do
    with {:ok, snapshot} <- Snapshot.load(channel_id) do
      :ets.insert(@table, {{channel_id, snapshot.acl_version}, snapshot, now()})
      {:ok, snapshot}
    end
  end

  @doc "Number of cached entries. For tests and metrics."
  @spec size() :: non_neg_integer()
  def size, do: :ets.info(@table, :size)

  @doc false
  def clear, do: :ets.delete_all_objects(@table)

  @impl GenServer
  def init(_opts) do
    :ets.new(@table, [:named_table, :set, :public, read_concurrency: true])
    Process.send_after(self(), :sweep, @sweep_ms)
    {:ok, %{}}
  end

  @impl GenServer
  def handle_info(:sweep, state) do
    cutoff = now() - @ttl_ms
    :ets.select_delete(@table, [{{:_, :_, :"$1"}, [{:<, :"$1", cutoff}], [true]}])
    Process.send_after(self(), :sweep, @sweep_ms)
    {:noreply, state}
  end

  defp now, do: System.monotonic_time(:millisecond)
end
