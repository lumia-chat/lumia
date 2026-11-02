defmodule Lumia.Servers.MemberRole do
  @moduledoc "A role held by a member. `@everyone` is implicit and never stored."

  use Lumia.Schema

  @primary_key false
  schema "member_roles" do
    field :server_id, Ecto.UUID, primary_key: true
    field :account_id, Ecto.UUID, primary_key: true
    field :role_id, Ecto.UUID, primary_key: true
  end
end
