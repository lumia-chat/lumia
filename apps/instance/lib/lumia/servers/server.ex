defmodule Lumia.Servers.Server do
  @moduledoc "A server: a community inside an instance (architecture 2)."

  use Lumia.Schema

  @type t :: %__MODULE__{}

  schema "servers" do
    field :name, :string
    field :icon_url, :string
    belongs_to :owner, Lumia.Accounts.Account, foreign_key: :owner_account_id
    timestamps()
  end
end
