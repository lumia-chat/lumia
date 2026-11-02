defmodule Lumia.InstanceTest do
  # Not async: persistent_term is global.
  use Lumia.DataCase, async: false

  alias Lumia.Instance

  test "recovery generation is loaded at boot and readable without the database" do
    assert is_integer(Instance.recovery_generation())
    assert Instance.recovery_generation() >= 1
  end

  test "bump increments and persists" do
    before = Instance.recovery_generation()
    assert Instance.bump_recovery_generation() == before + 1
    assert Instance.recovery_generation() == before + 1
    assert Instance.reload().recovery_generation == before + 1
  end

  test "the settings table holds exactly one row" do
    assert_raise Postgrex.Error, ~r/instance_settings_singleton|duplicate key/, fn ->
      Repo.query!(
        "INSERT INTO instance_settings (singleton, inserted_at, updated_at) VALUES (false, now(), now())"
      )
    end
  end
end
