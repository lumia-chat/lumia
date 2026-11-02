defmodule Lumia.History.Manifest do
  @moduledoc "One device's history manifest for this instance: the newest version it uploaded."

  use Lumia.Schema

  @primary_key false
  schema "history_manifests" do
    field :identity_id, :binary, primary_key: true
    field :device_id, Ecto.UUID, primary_key: true
    field :version, :integer
    field :manifest, :binary
    field :updated_at, :utc_datetime_usec
  end
end
