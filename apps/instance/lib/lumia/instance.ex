defmodule Lumia.Instance do
  @moduledoc """
  Instance-wide settings, loaded into `persistent_term` at boot.

  The recovery generation (docs/specs/delivery.md section 8) is sent in
  every `Welcome` and in `InstanceInfo`; reading it must not touch the
  database. Only `bump_recovery_generation/0` changes it, and only the
  restore and settings commands call that.
  """

  use GenServer

  alias Lumia.Instance.Settings
  alias Lumia.Repo

  @key {__MODULE__, :settings}

  def start_link(opts), do: GenServer.start_link(__MODULE__, opts, name: __MODULE__)

  @doc "The current recovery generation. Never hits the database."
  def recovery_generation, do: :persistent_term.get(@key).recovery_generation

  @doc """
  Increments the recovery generation. Every connected client discards its
  cursors on its next connect. Called by `lumia restore` and
  `lumia settings bump-generation`, never by application code.
  """
  def bump_recovery_generation do
    settings = Repo.one!(Settings)

    {:ok, updated} =
      settings
      |> Ecto.Changeset.change(recovery_generation: settings.recovery_generation + 1)
      |> Repo.update()

    :persistent_term.put(@key, updated)
    updated.recovery_generation
  end

  @doc "Reloads settings from the database into persistent_term."
  def reload do
    settings = Repo.one!(Settings)
    :persistent_term.put(@key, settings)
    settings
  end

  @impl GenServer
  def init(_opts) do
    reload()
    {:ok, %{}}
  end
end
