defmodule Lumia.Repo.Migrations.RolesAndTitles do
  @moduledoc """
  Roles and titles on the clients (architecture 7.5, authorization 3.5 and
  6.4): a role's tag switch, a member's title, the server's choice between
  the two, and the members other members are yet to be told of.

  A role's position becomes the instance's own and one role's alone in its
  server, and its name one role's alone whatever its capitals, so what is
  kept is brought to that first: `@everyone` is put last again, wherever
  an older release let a request put it; the other roles are numbered 1,
  2, 3 in the order they had; names are trimmed to 32 characters and a
  name held twice is numbered, with a number no other role's name ends up
  with; and a colour that is not a tone is let go.
  """

  use Ecto.Migration

  @tones ~w(peach blue purple mint amber sky stone green indigo)

  def up do
    alter table(:roles) do
      add :hide_tag, :boolean, null: false, default: false
    end

    alter table(:server_members) do
      add :title, :text
    end

    alter table(:servers) do
      add :member_tag, :text, null: false, default: "role"
    end

    create constraint(:servers, :servers_member_tag, check: "member_tag IN ('role', 'title')")

    create table(:member_announcements) do
      add :server_id, references(:servers, on_delete: :delete_all), null: false
      add :account_id, :uuid, null: false
    end

    create index(:member_announcements, [:server_id])

    execute "UPDATE roles SET position = 1000000 WHERE is_everyone"

    execute """
    UPDATE roles r SET position = t.n
    FROM (SELECT id, row_number() OVER (PARTITION BY server_id ORDER BY position, inserted_at, id) AS n
          FROM roles WHERE NOT is_everyone) t
    WHERE t.id = r.id
    """

    execute "UPDATE roles SET name = left(btrim(name), 32) WHERE NOT is_everyone"
    execute "UPDATE roles SET name = 'Role' WHERE NOT is_everyone AND name = ''"

    # The first to hold a name keeps it, `@everyone` before any other; each
    # later one takes the first number that leaves its name nobody's, read
    # again after every rename, so "Mod", "mod" and "Mod 2" end as three
    # names.
    execute """
    DO $$
    DECLARE
      doubled record;
      n integer;
      numbered text;
    BEGIN
      FOR doubled IN
        SELECT id, server_id, name
        FROM (SELECT id, server_id, name, position,
                     row_number() OVER (PARTITION BY server_id, lower(name)
                                        ORDER BY is_everyone DESC, position, id) AS place
              FROM roles) held
        WHERE place > 1
        ORDER BY server_id, position, id
      LOOP
        n := 2;
        LOOP
          numbered := left(doubled.name, 31 - length(n::text)) || ' ' || n;
          EXIT WHEN NOT EXISTS (
            SELECT 1 FROM roles
            WHERE server_id = doubled.server_id AND lower(name) = lower(numbered)
          );
          n := n + 1;
        END LOOP;
        UPDATE roles SET name = numbered WHERE id = doubled.id;
      END LOOP;
    END $$
    """

    execute "UPDATE roles SET color = NULL WHERE color IS NOT NULL AND color NOT IN (#{Enum.map_join(@tones, ", ", &"'#{&1}'")})"

    drop index(:roles, [:server_id, :position])

    # Deferrable, so it is checked when a statement ends and not row by
    # row: a reorder is one UPDATE in which roles trade places.
    execute """
    ALTER TABLE roles ADD CONSTRAINT roles_server_position
    UNIQUE (server_id, position) DEFERRABLE INITIALLY IMMEDIATE
    """

    create unique_index(:roles, [:server_id, "lower(name)"], name: :roles_server_name)
  end

  def down do
    drop index(:roles, [:server_id], name: :roles_server_name)
    execute "ALTER TABLE roles DROP CONSTRAINT roles_server_position"
    create index(:roles, [:server_id, :position])
    drop table(:member_announcements)
    drop constraint(:servers, :servers_member_tag)

    alter table(:servers) do
      remove :member_tag
    end

    alter table(:server_members) do
      remove :title
    end

    alter table(:roles) do
      remove :hide_tag
    end
  end
end
