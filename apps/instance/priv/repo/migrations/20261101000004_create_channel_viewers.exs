defmodule Lumia.Repo.Migrations.CreateChannelViewers do
  @moduledoc """
  The last-computed set of accounts that can view each channel, maintained
  by the viewer-diff job (docs/specs/authorization.md section 6.3). The
  diff between this and the freshly resolved set is what produces
  ChannelAdded and ChannelRemoved on account logs.
  """

  use Ecto.Migration

  def change do
    create table(:channel_viewers, primary_key: false) do
      add :channel_id, references(:channels, on_delete: :delete_all),
        null: false,
        primary_key: true

      add :account_id, references(:accounts, on_delete: :delete_all),
        null: false,
        primary_key: true
    end

    create index(:channel_viewers, [:account_id])
  end
end
