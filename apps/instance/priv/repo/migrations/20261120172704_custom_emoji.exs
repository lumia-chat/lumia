defmodule Lumia.Repo.Migrations.CustomEmoji do
  @moduledoc """
  A server's custom emoji (specs/emoji.md): a row each, kept when removed
  so that reactions with it can still be named; the server's own limit,
  set by the operator (nil for the instance's default); the emoji a
  message's body uses, as byte ranges beside it as its mentions are; and a
  reaction's emoji when it is one of these.
  """

  use Ecto.Migration

  def change do
    create table(:custom_emoji) do
      add :server_id, references(:servers, on_delete: :delete_all), null: false
      add :name, :text, null: false
      add :image_url, :text
      add :animated, :boolean, null: false, default: false
      add :width, :integer, null: false
      add :height, :integer, null: false
      add :created_by_account_id, references(:accounts, on_delete: :nilify_all)
      add :deleted_at, :utc_datetime_usec
      timestamps()
    end

    # A name is one live emoji's alone in its server.
    create unique_index(:custom_emoji, [:server_id, :name],
             where: "deleted_at IS NULL",
             name: :custom_emoji_live_name
           )

    alter table(:servers) do
      add :emoji_limit, :integer
    end

    alter table(:messages) do
      add :emoji_spans, {:array, :map}, null: false, default: []
    end

    alter table(:message_reactions) do
      add :emoji_id, :uuid
    end
  end
end
