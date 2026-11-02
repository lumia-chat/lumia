defmodule Lumia.Mls.KeyPackage do
  @moduledoc "One unclaimed MLS key package of a device, as opaque bytes (e2ee spec section 3)."

  use Lumia.Schema

  schema "mls_key_packages" do
    field :account_id, Ecto.UUID
    field :device_id, Ecto.UUID
    field :key_package, :binary
    field :last_resort, :boolean, default: false
    field :expires_at, :utc_datetime_usec
    field :inserted_at, :utc_datetime_usec
  end
end
