defmodule Lumia.Servers.Role do
  @moduledoc """
  A role: a named permission set with a position. `permissions` is a
  bitfield whose bit index equals the proto `Permission` enum value; the API
  never sees it as a number (docs/specs/authorization.md section 2).
  `position` is the instance's to set (section 3.5): lower is higher, no
  two roles of a server share one, and `@everyone` is last. `color` is a
  tone's name or nil; `hide_tag` keeps the role from being a member's tag.
  """

  use Lumia.Schema

  schema "roles" do
    field :server_id, Ecto.UUID
    field :name, :string
    field :position, :integer, default: 0
    field :permissions, :integer, default: 0
    field :color, :string
    field :hide_tag, :boolean, default: false
    field :is_everyone, :boolean, default: false
    timestamps()
  end
end
