defmodule Lumia.Audit.Entry do
  @moduledoc "One audit-log row."

  use Lumia.Schema

  @type t :: %__MODULE__{}

  schema "audit_log" do
    field :scope_kind, :string
    field :scope_id, Ecto.UUID
    field :actor_account_id, Ecto.UUID
    field :action, :string
    field :target_kind, :string
    field :target_id, Ecto.UUID
    field :before, :map
    field :after, :map
    field :override, :boolean, default: false
    field :created_at, :utc_datetime_usec
  end
end
