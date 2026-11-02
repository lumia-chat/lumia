defmodule Lumia.Repo.Migrations.CreateServersAndAuthorization do
  @moduledoc """
  Servers, memberships, roles, overwrites, and the audit log
  (docs/specs/authorization.md section 2).
  """

  use Ecto.Migration

  def change do
    create table(:servers) do
      add :name, :text, null: false
      add :icon_url, :text
      add :owner_account_id, references(:accounts, on_delete: :restrict), null: false
      timestamps()
    end

    create index(:servers, [:owner_account_id])

    alter table(:channels) do
      add :server_id, references(:servers, on_delete: :delete_all)
      add :name, :text
      add :topic, :text
      add :position, :integer, null: false, default: 0
      add :deleted_at, :utc_datetime_usec
    end

    create index(:channels, [:server_id, :position])

    create table(:server_members, primary_key: false) do
      add :server_id, references(:servers, on_delete: :delete_all), null: false, primary_key: true

      add :account_id, references(:accounts, on_delete: :delete_all),
        null: false,
        primary_key: true

      add :kind, :text, null: false, default: "member"
      add :nickname, :text
      add :joined_at, :utc_datetime_usec, null: false
      add :expires_at, :utc_datetime_usec
    end

    create index(:server_members, [:account_id])
    create index(:server_members, [:expires_at], where: "expires_at IS NOT NULL")
    create constraint(:server_members, :server_members_kind, check: "kind IN ('member', 'guest')")

    create table(:roles) do
      add :server_id, references(:servers, on_delete: :delete_all), null: false
      add :name, :text, null: false
      add :position, :integer, null: false, default: 0
      # Bit index equals the proto Permission enum value.
      add :permissions, :bigint, null: false, default: 0
      add :color, :text
      add :is_everyone, :boolean, null: false, default: false
      timestamps()
    end

    create index(:roles, [:server_id, :position])

    create unique_index(:roles, [:server_id],
             where: "is_everyone",
             name: :roles_one_everyone_per_server
           )

    create table(:member_roles, primary_key: false) do
      add :server_id, :binary_id, null: false, primary_key: true
      add :account_id, :binary_id, null: false, primary_key: true
      add :role_id, references(:roles, on_delete: :delete_all), null: false, primary_key: true
    end

    execute(
      "ALTER TABLE member_roles ADD CONSTRAINT member_roles_membership_fkey FOREIGN KEY (server_id, account_id) REFERENCES server_members (server_id, account_id) ON DELETE CASCADE",
      "ALTER TABLE member_roles DROP CONSTRAINT member_roles_membership_fkey"
    )

    create index(:member_roles, [:role_id])

    create table(:channel_overwrites, primary_key: false) do
      add :channel_id, references(:channels, on_delete: :delete_all),
        null: false,
        primary_key: true

      add :target_kind, :text, null: false, primary_key: true
      add :target_id, :binary_id, null: false, primary_key: true
      add :allow, :bigint, null: false, default: 0
      add :deny, :bigint, null: false, default: 0
    end

    create constraint(:channel_overwrites, :channel_overwrites_target_kind,
             check: "target_kind IN ('role', 'member')"
           )

    create table(:audit_log) do
      add :scope_kind, :text, null: false
      add :scope_id, :binary_id
      add :actor_account_id, :binary_id
      add :action, :text, null: false
      add :target_kind, :text
      add :target_id, :binary_id
      add :before, :jsonb
      add :after, :jsonb
      add :override, :boolean, null: false, default: false
      add :created_at, :utc_datetime_usec, null: false
    end

    create index(:audit_log, [:scope_kind, :scope_id, :created_at])

    create constraint(:audit_log, :audit_log_scope_kind,
             check: "scope_kind IN ('server', 'instance')"
           )
  end
end
