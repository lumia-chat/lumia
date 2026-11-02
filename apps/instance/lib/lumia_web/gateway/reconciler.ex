defmodule LumiaWeb.Gateway.Reconciler do
  @moduledoc """
  Per-node reconciliation (docs/specs/delivery.md section 7).

  Every tick, queries the channels and accounts whose log head moved since
  the previous tick (with a one-second overlap) and does a node-local
  broadcast of the head to their topics. Sessions on this node that are
  behind run their replay loop. Topics with no local subscribers cost one
  ETS lookup. This closes the commit-then-die gap within one tick and
  needs no outbox.
  """

  use GenServer

  alias Lumia.Accounts
  alias Lumia.Events

  @tick_ms 5_000
  @overlap_seconds 1

  def start_link(opts), do: GenServer.start_link(__MODULE__, opts, name: __MODULE__)

  @doc "Runs one reconciliation pass now. Returns how many heads were broadcast."
  def tick(since \\ nil) do
    since =
      since ||
        DateTime.add(DateTime.utc_now(), -(div(@tick_ms, 1_000) + @overlap_seconds), :second)

    channels = Events.changed_since(since)
    accounts = Accounts.changed_since(since)

    Enum.each(channels, fn {channel_id, seq} ->
      Phoenix.PubSub.local_broadcast(
        Lumia.PubSub,
        Events.topic(channel_id),
        {:head, channel_id, seq}
      )
    end)

    Enum.each(accounts, fn {account_id, seq} ->
      Phoenix.PubSub.local_broadcast(
        Lumia.PubSub,
        Accounts.topic(account_id),
        {:account_head, seq}
      )
    end)

    length(channels) + length(accounts)
  end

  @impl GenServer
  def init(_opts) do
    schedule()
    {:ok, %{last_tick: DateTime.utc_now()}}
  end

  @impl GenServer
  def handle_info(:tick, state) do
    since = DateTime.add(state.last_tick, -@overlap_seconds, :second)
    now = DateTime.utc_now()
    tick(since)
    schedule()
    {:noreply, %{state | last_tick: now}}
  end

  defp schedule, do: Process.send_after(self(), :tick, @tick_ms)
end
