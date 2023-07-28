defmodule Lumia.Schema do
  @moduledoc """
  Lumia.Schema is a base schema for all Lumia schemas.
  """

  defmacro __using__(_opts) do
    quote do
      use Ecto.Schema
      @primary_key {:id, Uniq.UUID, version: 7, autogenerate: true}
      @foreign_key_type Uniq.UUID
      @timestamps_opts [type: :utc_datetime]
    end
  end
end
