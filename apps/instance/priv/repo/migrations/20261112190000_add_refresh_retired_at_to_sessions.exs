defmodule Lumia.Repo.Migrations.AddRefreshRetiredAtToSessions do
  @moduledoc """
  When the session's previous refresh token was rotated away, so one
  presented again within the grace gets the same answer and one presented
  after it is a reuse (docs/protocol.md section 12).
  """

  use Ecto.Migration

  def change do
    alter table(:sessions) do
      add :previous_refresh_retired_at, :utc_datetime_usec
    end
  end
end
