defmodule Lumia.Servers.Server do
  @moduledoc "A server: a community inside an instance (architecture 2)."

  use Lumia.Schema

  schema "servers" do
    field :name, :string
    field :icon_url, :string
    field :retention_days, :integer
    belongs_to :owner, Lumia.Accounts.Account, foreign_key: :owner_account_id
    timestamps()
  end
end
