defmodule Lumia.Workers.PurgeCommands do
  @moduledoc """
  Deletes `commands` rows older than the idempotency retention window
  (docs/specs/delivery.md section 9). Runs daily from the Oban cron.

  A retry older than the window is a client bug; a message send still
  cannot duplicate because `messages.id` is the command id.
  """

  use Oban.Worker, queue: :maintenance, max_attempts: 3

  import Ecto.Query

  alias Lumia.Events.Command
  alias Lumia.Repo

  @retention_days 30
  @batch 10_000

  @impl Oban.Worker
  def perform(%Oban.Job{}) do
    {:ok, purge(cutoff(), 0)}
  end

  @doc "The cutoff instant: rows created before it are purged."
  @spec cutoff() :: DateTime.t()
  def cutoff, do: DateTime.add(DateTime.utc_now(), -@retention_days, :day)

  # Deletes in batches so a long-neglected instance does not hold one giant
  # transaction. Partition pruning does not apply (the predicate is on
  # created_at, not channel_id), but the per-partition created_at index does.
  defp purge(cutoff, deleted) do
    {n, _} =
      Repo.delete_all(
        from c in Command,
          where:
            fragment(
              "(?, ?) IN (SELECT channel_id, command_id FROM commands WHERE created_at < ? LIMIT ?)",
              c.channel_id,
              c.command_id,
              ^cutoff,
              @batch
            )
      )

    if n < @batch, do: deleted + n, else: purge(cutoff, deleted + n)
  end
end
