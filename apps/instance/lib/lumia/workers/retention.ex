defmodule Lumia.Workers.Retention do
  @moduledoc """
  Applies per-server retention (docs/specs/delivery.md section 10).

  For every server with `retention_days`, and every channel in it, finds
  the newest event older than the policy and purges everything at or
  below it: payloads are nulled (the event rows stay, so ordering and gap
  detection hold), materialized messages are deleted, and one
  `PayloadsPurged{before_seq}` event is appended so online clients drop
  cached bodies. Resumed clients see `Redacted` tombstones at the original
  sequence numbers.

  Daily from the Oban cron; idempotent, so a rerun purges nothing new.
  """

  use Oban.Worker, queue: :maintenance, max_attempts: 3, unique: [period: 3600]

  import Ecto.Query

  alias Lumia.Events
  alias Lumia.Events.Channel
  alias Lumia.Repo
  alias Lumia.Servers.Server

  @impl Oban.Worker
  def perform(%Oban.Job{}) do
    purged =
      from(s in Server, where: not is_nil(s.retention_days), select: {s.id, s.retention_days})
      |> Repo.all()
      |> Enum.map(fn {server_id, days} -> apply_to_server(server_id, days) end)
      |> Enum.sum()

    {:ok, purged}
  end

  @doc "Purges one server's channels now. Returns how many channels were purged."
  def apply_to_server(server_id, days) when is_integer(days) and days > 0 do
    cutoff = DateTime.add(DateTime.utc_now(), -days, :day)

    from(c in Channel, where: c.server_id == ^server_id and is_nil(c.deleted_at), select: c.id)
    |> Repo.all()
    |> Enum.count(fn channel_id ->
      case Events.purge_before(channel_id, cutoff) do
        {:ok, 0} -> false
        {:ok, _n} -> true
        {:error, _} -> false
      end
    end)
  end
end
