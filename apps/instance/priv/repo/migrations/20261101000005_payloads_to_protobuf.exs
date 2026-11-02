defmodule Lumia.Repo.Migrations.PayloadsToProtobuf do
  @moduledoc """
  Event payloads become serialized protobuf: a stored payload is the exact
  `Lumia.V1.Event` (or `AccountEvent`) bytes that get broadcast and sent on
  the wire, so an event is encoded once (docs/specs/delivery.md section 4).
  Pre-release; existing jsonb rows are dropped.
  """

  use Ecto.Migration

  def up do
    execute "ALTER TABLE event_payloads ALTER COLUMN payload TYPE bytea USING NULL"
    execute "ALTER TABLE account_event_payloads ALTER COLUMN payload TYPE bytea USING NULL"
  end

  def down do
    execute "ALTER TABLE event_payloads ALTER COLUMN payload TYPE jsonb USING NULL"
    execute "ALTER TABLE account_event_payloads ALTER COLUMN payload TYPE jsonb USING NULL"
  end
end
