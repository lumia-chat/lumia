defmodule Lumia.Servers.Server do
  @moduledoc "A server: a community inside an instance (architecture 2)."

  use Lumia.Schema

  schema "servers" do
    field :name, :string
    field :icon_url, :string
    field :retention_days, :integer
    field :show_deleted, :boolean, default: false
    # Its files (specs/media.md 4.5): the bytes its blobs take, kept by
    # `Lumia.Uploads`; how long attachments are kept (nil for ever); and
    # what happens when it is full, `"refuse"` or `"delete_oldest"`.
    field :stored_bytes, :integer, default: 0
    field :attachment_retention_days, :integer
    field :storage_full, :string, default: "refuse"
    belongs_to :owner, Lumia.Accounts.Account, foreign_key: :owner_account_id
    timestamps()
  end
end
