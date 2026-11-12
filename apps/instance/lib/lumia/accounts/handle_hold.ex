defmodule Lumia.Accounts.HandleHold do
  @moduledoc """
  The handle an account last left, held for it until `until`
  (`Lumia.Accounts.update_account/2`): nobody else can take it meanwhile,
  so a rename cannot be followed at once by someone posing under the old
  name. One row per account.
  """

  use Lumia.Schema

  @primary_key {:account_id, Ecto.UUID, autogenerate: false}
  schema "handle_holds" do
    field :handle, :string
    field :until, :utc_datetime_usec
  end
end
