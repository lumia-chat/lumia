defmodule Lumia.Repo.Migrations.CreateAccountsAndInstance do
  @moduledoc """
  Accounts, the account log, and instance settings
  (docs/specs/delivery.md section 2, docs/specs/authorization.md section 2).
  """

  use Ecto.Migration

  @partitions 32

  def change do
    # Single row. `singleton` can only ever be true, and it is the primary key.
    create table(:instance_settings, primary_key: false) do
      add :singleton, :boolean, null: false, default: true, primary_key: true
      add :recovery_generation, :bigint, null: false, default: 1
      timestamps()
    end

    create constraint(:instance_settings, :instance_settings_singleton, check: "singleton")

    execute(
      "INSERT INTO instance_settings (singleton, recovery_generation, inserted_at, updated_at) VALUES (true, 1, now(), now())",
      "SELECT 1"
    )

    # Same HOT rules as channels: fillfactor and a second-granularity
    # last_seq_at, because every account-log append updates this row.
    create table(:accounts, options: "WITH (fillfactor = 70)") do
      add :identity_id, :binary
      add :kind, :text, null: false, default: "human"
      add :handle, :text, null: false
      add :display_name, :text
      add :avatar_url, :text
      add :email, :text
      add :password_hash, :text
      add :instance_role, :text, null: false, default: "none"
      add :last_seq, :bigint, null: false, default: 0
      add :last_seq_at, :utc_datetime
      add :suspended_at, :utc_datetime_usec
      add :deleted_at, :utc_datetime_usec
      timestamps()
    end

    create unique_index(:accounts, ["lower(handle)"], name: :accounts_handle_index)

    create unique_index(:accounts, ["lower(email)"],
             name: :accounts_email_index,
             where: "email IS NOT NULL"
           )

    create index(:accounts, [:identity_id], where: "identity_id IS NOT NULL")
    create index(:accounts, [:last_seq_at])
    create constraint(:accounts, :accounts_kind, check: "kind IN ('human', 'bot')")

    create constraint(:accounts, :accounts_instance_role,
             check: "instance_role IN ('none', 'moderator', 'admin', 'owner')"
           )

    create table(:account_events, primary_key: false, options: "PARTITION BY HASH (account_id)") do
      add :account_id, :binary_id, null: false, primary_key: true
      add :seq, :bigint, null: false, primary_key: true
      add :id, :binary_id, null: false
      add :kind, :text, null: false
      add :created_at, :utc_datetime_usec, null: false
    end

    create table(:account_event_payloads,
             primary_key: false,
             options: "PARTITION BY HASH (account_id)"
           ) do
      add :account_id, :binary_id, null: false, primary_key: true
      add :seq, :bigint, null: false, primary_key: true
      add :payload, :jsonb
    end

    for table <- ~w(account_events account_event_payloads), i <- 0..(@partitions - 1) do
      execute(
        "CREATE TABLE #{table}_p#{i} PARTITION OF #{table} FOR VALUES WITH (MODULUS #{@partitions}, REMAINDER #{i})",
        "SELECT 1"
      )
    end

    # Purge job lookup (delivery section 9). Per-partition indexes are
    # created automatically on a partitioned table.
    create index(:commands, [:created_at])
  end
end
