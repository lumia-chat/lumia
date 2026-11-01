defmodule Lumia.Schema do
  @moduledoc """
  Base for every Ecto schema in the instance.

  Sets the conventions from `docs/architecture.md` section 8.1:

    * primary keys are UUIDv7 minted app-side with Ecto's monotonic
      precision, strictly increasing per node;
    * foreign keys are UUIDs;
    * timestamps are UTC with microsecond precision, so they round-trip to
      `google.protobuf.Timestamp` without loss.

  Schemas whose primary key is not a single `id` column (the partitioned
  event-log tables) set `@primary_key false` after `use Lumia.Schema` and
  mint their ids with `generate_id/0`.
  """

  defmacro __using__(_opts) do
    quote do
      use Ecto.Schema

      @primary_key {:id, Ecto.UUID, autogenerate: [version: 7, precision: :monotonic]}
      @foreign_key_type Ecto.UUID
      @timestamps_opts [type: :utc_datetime_usec]
    end
  end

  @doc """
  Mints a server-side id: UUIDv7, monotonic per node.

  Never derive a time from one of these. The monotonic counter can run ahead
  of the wall clock under burst; `created_at` is always its own column.
  """
  @spec generate_id() :: Ecto.UUID.t()
  def generate_id, do: Ecto.UUID.generate(version: 7, precision: :monotonic)
end
