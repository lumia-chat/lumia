defmodule Lumia.Repo.Migrations.CreateLinkSessions do
  @moduledoc """
  Linking by prompt (specs/identity.md 4.2.1): the new device's request
  and commitment, the answer of the device that holds the identity, the
  reveal, and the link bundle's code, relayed for ten minutes.
  """

  use Ecto.Migration

  def change do
    create table(:link_sessions) do
      add :account_id, references(:accounts, on_delete: :delete_all), null: false
      # The session that opened it (the new device) and the one that answered.
      add :opened_by, references(:sessions, on_delete: :delete_all), null: false
      add :answered_by, references(:sessions, on_delete: :nilify_all)
      add :request, :binary, null: false
      add :commitment, :binary, null: false
      add :answer, :binary
      add :reveal, :binary
      add :code, :string
      add :declined_at, :utc_datetime_usec
      add :expires_at, :utc_datetime_usec, null: false
      timestamps()
    end

    create index(:link_sessions, [:account_id, :expires_at])
    create index(:link_sessions, [:expires_at])
  end
end
