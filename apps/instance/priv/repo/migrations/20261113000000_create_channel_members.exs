defmodule Lumia.Repo.Migrations.CreateChannelMembers do
  @moduledoc """
  Channel membership on top of permission (architecture 6.8): who is in
  each channel, the invitations waiting for those who take adds as
  invitations, which channels a newcomer joins, whether a channel was
  made private, each member's two switches, and the notices a channel
  shows for joining, being added and leaving.

  The conversion is the one the architecture states: every member is put
  in every channel they could see (`channel_viewers`, which the viewer
  diff kept as exactly that), and each server's first channel becomes its
  default. Members may start server DMs by default, so every server's
  `@everyone` is given `START_SERVER_DMS` (bit 21).
  """

  use Ecto.Migration

  def up do
    create table(:channel_members, primary_key: false) do
      add :channel_id, references(:channels, on_delete: :delete_all),
        null: false,
        primary_key: true

      add :account_id, references(:accounts, on_delete: :delete_all),
        null: false,
        primary_key: true

      add :added_by_account_id, references(:accounts, on_delete: :nilify_all)
      add :joined_at, :utc_datetime_usec, null: false
    end

    create index(:channel_members, [:account_id])

    create table(:channel_invitations, primary_key: false) do
      add :channel_id, references(:channels, on_delete: :delete_all),
        null: false,
        primary_key: true

      add :account_id, references(:accounts, on_delete: :delete_all),
        null: false,
        primary_key: true

      add :invited_by_account_id, references(:accounts, on_delete: :nilify_all)
      add :created_at, :utc_datetime_usec, null: false
    end

    create index(:channel_invitations, [:account_id])

    alter table(:channels) do
      add :private, :boolean, null: false, default: false
      add :is_default, :boolean, null: false, default: false
    end

    alter table(:server_members) do
      add :channel_adds_by_invitation, :boolean, null: false, default: false
      add :server_dms_off, :boolean, null: false, default: false
    end

    alter table(:messages) do
      add :notice, :text
      add :notice_account_id, :binary_id
    end

    execute """
    INSERT INTO channel_members (channel_id, account_id, joined_at)
    SELECT v.channel_id, v.account_id, now()
    FROM channel_viewers v
    JOIN channels c ON c.id = v.channel_id
    WHERE c.server_id IS NOT NULL
    ON CONFLICT DO NOTHING
    """

    # The viewer diff runs debounced, so a member who arrived just before
    # this ran may not be in `channel_viewers` yet; in a channel with no
    # overwrites, every member can see it.
    execute """
    INSERT INTO channel_members (channel_id, account_id, joined_at)
    SELECT c.id, m.account_id, now()
    FROM channels c
    JOIN server_members m ON m.server_id = c.server_id AND m.kind = 'member'
    WHERE c.kind = 'text' AND c.deleted_at IS NULL
      AND NOT EXISTS (SELECT 1 FROM channel_overwrites o WHERE o.channel_id = c.id)
    ON CONFLICT DO NOTHING
    """

    execute "UPDATE roles SET permissions = permissions | (1::bigint << 21) WHERE is_everyone"

    execute """
    UPDATE channels SET is_default = true
    WHERE id IN (
      SELECT DISTINCT ON (server_id) id FROM channels
      WHERE server_id IS NOT NULL AND kind = 'text' AND deleted_at IS NULL
      ORDER BY server_id, position, id
    )
    """
  end

  def down do
    alter table(:messages) do
      remove :notice
      remove :notice_account_id
    end

    alter table(:server_members) do
      remove :channel_adds_by_invitation
      remove :server_dms_off
    end

    alter table(:channels) do
      remove :private
      remove :is_default
    end

    drop table(:channel_invitations)
    drop table(:channel_members)
  end
end
