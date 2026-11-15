defmodule Lumia.Repo.Migrations.AddDeviceRevokedToSessions do
  @moduledoc """
  Whether a session was revoked because its device was revoked in the
  identity's roster: its socket closes with `DISCONNECT_DEVICE_REVOKED` and
  its calls answer `DEVICE_REVOKED`, so the device erases what it kept
  (identity spec 4.4), where a session merely signed out does not.
  """

  use Ecto.Migration

  def change do
    alter table(:sessions) do
      add :device_revoked, :boolean, null: false, default: false
    end
  end
end
