defmodule Lumia.Repo.Migrations.CreateEventLog do
  @moduledoc """
  The event log from `docs/architecture.md` section 8.

  `channels` owns the per-channel sequence. `events`, `event_payloads` and
  `commands` are hash-partitioned by `channel_id` so that the sync query
  `(channel_id, seq > $n)` and the idempotency lookup `(channel_id, command_id)`
  each prune to one partition. `messages` is the materialized view of
  message events that reads go through.
  """

  use Ecto.Migration

  @partitions 32

  def change do
    # Every write updates this row. fillfactor leaves room on the page so the
    # update can be HOT (no new index entry) as long as no indexed column
    # changed, which is why last_seq_at is truncated to the second on write.
    create table(:channels, options: "WITH (fillfactor = 70)") do
      add :kind, :text, null: false, default: "text"
      add :last_seq, :bigint, null: false, default: 0
      # Second granularity on purpose: reconciliation does not need better,
      # and an indexed column that changed on every write would defeat HOT.
      add :last_seq_at, :utc_datetime
      add :acl_version, :bigint, null: false, default: 1
      timestamps()
    end

    # Reconciliation: "which channels changed since the last tick".
    create index(:channels, [:last_seq_at])

    # Event rows are permanent; payloads live in event_payloads and are
    # nulled by retention and deletion (architecture 8.5).
    create table(:events, primary_key: false, options: "PARTITION BY HASH (channel_id)") do
      add :channel_id, :binary_id, null: false, primary_key: true
      add :seq, :bigint, null: false, primary_key: true
      add :id, :binary_id, null: false
      add :kind, :text, null: false
      add :actor_account_id, :binary_id
      add :actor_device_id, :binary_id
      add :command_id, :binary_id
      add :acl_version, :bigint, null: false
      add :created_at, :utc_datetime_usec, null: false
    end

    create table(:event_payloads,
             primary_key: false,
             options: "PARTITION BY HASH (channel_id)"
           ) do
      add :channel_id, :binary_id, null: false, primary_key: true
      add :seq, :bigint, null: false, primary_key: true
      # jsonb for the spike; becomes bytea (serialized protobuf) when the
      # event payload types land.
      add :payload, :jsonb
    end

    # Idempotency: one row per accepted command, holding what the ack said.
    create table(:commands, primary_key: false, options: "PARTITION BY HASH (channel_id)") do
      add :channel_id, :binary_id, null: false, primary_key: true
      add :command_id, :binary_id, null: false, primary_key: true
      add :actor_account_id, :binary_id
      add :fingerprint, :bytea, null: false
      add :seq, :bigint, null: false
      add :event_id, :binary_id, null: false
      add :created_at, :utc_datetime_usec, null: false
    end

    for table <- ~w(events event_payloads commands), i <- 0..(@partitions - 1) do
      execute(
        "CREATE TABLE #{table}_p#{i} PARTITION OF #{table} FOR VALUES WITH (MODULUS #{@partitions}, REMAINDER #{i})",
        # Dropping the parent drops its partitions.
        "SELECT 1"
      )
    end

    create table(:messages) do
      add :channel_id, references(:channels, on_delete: :delete_all), null: false
      add :seq, :bigint, null: false
      add :author_account_id, :binary_id
      add :parent_id, :binary_id
      add :body, :text
      add :created_at, :utc_datetime_usec, null: false
      add :edited_at, :utc_datetime_usec
      add :deleted_at, :utc_datetime_usec
    end

    create unique_index(:messages, [:channel_id, :seq])
    create index(:messages, [:channel_id, :parent_id], where: "parent_id IS NOT NULL")
  end
end
