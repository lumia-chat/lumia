defmodule Lumia.Repo.Migrations.AddServerRetention do
  @moduledoc """
  Per-server retention (architecture 6.6, docs/specs/delivery.md section
  10): content older than `retention_days` is purged by the retention job.
  Null means forever. Governance policy, not an entitlement.
  """

  use Ecto.Migration

  def change do
    alter table(:servers) do
      add :retention_days, :integer
    end

    create constraint(:servers, :servers_retention_days_positive,
             check: "retention_days IS NULL OR retention_days > 0"
           )
  end
end
