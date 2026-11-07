defmodule Lumia.FailureInjectionTest do
  @moduledoc """
  The named failure-injection suite (docs/plan.md, step 2 and step 8):
  the lists of delivery section 12, authorization section 11 and identity
  section 12, run as listed. The tests themselves live beside the code
  they exercise and carry the `failure_injection` tag; this module checks
  that every listed case has one, or is named below with the reason it
  has none yet.

      mix test.failure_injection

  runs the tagged tests, the two-node cases included, and this check.
  """

  # Not async: the sync modules run once every test file is loaded, which
  # the coverage check needs.
  use ExUnit.Case, async: false

  alias Lumia.FailureInjection

  @moduletag failure_injection: []

  # Cases whose instance-side test exists but whose client half is checked
  # elsewhere: the Rust core's own tests and the CLI scenarios.
  @elsewhere %{
    {:delivery, 7} =>
      "lumia-core sync::tests::generation_change_discards_everything_and_resumes_from_zero",
    {:delivery, 11} => "lumia-cli scenario snapshot-under-writes",
    {:identity, 1} => "lumia-cli scenario device-revoke",
    {:identity, 3} =>
      "lumia-core identity::roster::tests (the merge), lumia-client identity::tests::a_held_fork_at_the_same_version_is_merged_and_a_newer_successor_adopted, deploy/two-instances.sh (the convergence)",
    {:identity, 11} =>
      "lumia-cli scenario device-revoke (the phrase); lumia-core mls::group::tests::future_epoch_messages_are_held_and_group_info_admits_an_external_join (the rejoin)",
    {:identity, 12} =>
      "lumia-core identity::safety::tests::a_root_rotation_changes_the_safety_number (the safety number); the verifier and the instance's acceptance are tagged"
  }

  # Cases with no test yet, each with the reason. Shrinking this map is the
  # work; a case may not be both pending and covered.
  @pending %{
    {:authorization, 7} => "bot tokens and their scopes (authorization section 8) are not built",
    {:authorization, 8} =>
      "the NOT_FOUND half is covered; the override endpoints (authorization section 7) are not built"
  }

  # The pending cases whose instance half is covered and tagged anyway.
  @partly_covered [{:authorization, 8}]

  test "every listed case has a tagged test, or is named as pending with its reason" do
    cases = FailureInjection.cases()
    assert length(cases) == 35, "the three lists hold 35 cases"

    assert_whole_suite_loaded()

    covered = FailureInjection.covered()
    keys = MapSet.new(cases, fn {spec, n, _} -> {spec, n} end)

    for key <- Map.keys(covered), key not in keys do
      flunk("a test is tagged with #{inspect(key)}, which no spec lists")
    end

    for {key, _} <- @pending, key not in keys do
      flunk("#{inspect(key)} is pending but no spec lists it")
    end

    for {key, _} <- @elsewhere, key not in keys do
      flunk("#{inspect(key)} is named as covered elsewhere but no spec lists it")
    end

    for key <- @partly_covered do
      assert Map.has_key?(@pending, key) and Map.has_key?(covered, key),
             "#{inspect(key)} is listed as partly covered: it needs a pending reason and a tag"
    end

    for {key, _} <- @pending, key not in @partly_covered, Map.has_key?(covered, key) do
      flunk("#{inspect(key)} is covered now; drop it from the pending list")
    end

    missing =
      for {spec, n, text} <- cases,
          not Map.has_key?(covered, {spec, n}),
          not Map.has_key?(@pending, {spec, n}),
          do: "#{spec} #{n}: #{text}"

    assert missing == [], "cases without a tagged test:\n" <> Enum.join(missing, "\n")
  end

  test "the lists are read as the specs write them" do
    cases = FailureInjection.cases()

    assert {:delivery, 1, "Commit an event, kill the session process before broadcast; " <> _} =
             hd(cases)

    assert {:identity, 13, "Present a WEB device proof from an origin other than " <> _} =
             List.last(cases)

    assert Enum.count(cases, &match?({:authorization, _, _}, &1)) == 10
  end

  defp assert_whole_suite_loaded do
    on_disk =
      Path.wildcard(Path.expand("../**/*_test.exs", __DIR__))
      |> MapSet.new()

    loaded = MapSet.new(FailureInjection.loaded_test_files())
    not_loaded = MapSet.difference(on_disk, loaded)

    assert MapSet.size(not_loaded) == 0,
           "the coverage check needs every test file loaded (run mix test or " <>
             "mix test.failure_injection); not loaded: " <>
             Enum.map_join(not_loaded, ", ", &Path.relative_to_cwd/1)
  end
end
