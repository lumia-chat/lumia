defmodule Lumia.Auth.Session do
  @moduledoc "A session: one device's authorization on this instance."

  use Lumia.Schema

  schema "sessions" do
    field :account_id, Ecto.UUID
    field :device_id, Ecto.UUID
    field :access_hash, :binary, redact: true
    field :refresh_hash, :binary, redact: true
    field :previous_refresh_hash, :binary, redact: true
    field :client, :string
    field :ip, :string
    field :access_expires_at, :utc_datetime_usec
    field :refresh_expires_at, :utc_datetime_usec
    field :last_seen_at, :utc_datetime_usec
    field :revoked_at, :utc_datetime_usec
    timestamps()
  end
end
