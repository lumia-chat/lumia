defmodule Lumia.Servers.Role do
  @moduledoc """
  A role: a named permission set with a position. `permissions` is a
  bitfield whose bit index equals the proto `Permission` enum value; the API
  never sees it as a number (docs/specs/authorization.md section 2).
  """

  use Lumia.Schema

  schema "roles" do
    field :server_id, Ecto.UUID
    field :name, :string
    field :position, :integer, default: 0
    field :permissions, :integer, default: 0
    field :color, :string
    field :is_everyone, :boolean, default: false
    timestamps()
  end
end
