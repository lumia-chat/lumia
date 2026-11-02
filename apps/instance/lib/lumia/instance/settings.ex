defmodule Lumia.Instance.Settings do
  @moduledoc "The single instance_settings row."

  use Lumia.Schema

  @primary_key {:singleton, :boolean, autogenerate: false}
  schema "instance_settings" do
    field :recovery_generation, :integer, default: 1
    timestamps()
  end
end
