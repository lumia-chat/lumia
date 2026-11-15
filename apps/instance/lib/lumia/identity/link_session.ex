defmodule Lumia.Identity.LinkSession do
  @moduledoc "A link session (specs/identity.md 4.2.1); `Lumia.Identity.LinkSessions` keeps them."
  use Lumia.Schema

  schema "link_sessions" do
    field :account_id, Ecto.UUID
    field :opened_by, Ecto.UUID
    field :answered_by, Ecto.UUID
    field :request, :binary
    field :commitment, :binary
    field :answer, :binary
    field :reveal, :binary
    field :code, :string
    field :declined_at, :utc_datetime_usec
    field :expires_at, :utc_datetime_usec
    timestamps()
  end
end
