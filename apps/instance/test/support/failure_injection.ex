defmodule Lumia.FailureInjection do
  @moduledoc """
  The failure-injection lists the specs owe tests for, read from the specs
  themselves: delivery section 12, authorization section 11 and identity
  section 12. Each case is `{spec, n, text}`, numbered in the order the
  spec lists it.

  A test covers a case by carrying the tag
  `failure_injection: [delivery: 5]` (a keyword list; one test may cover
  several cases). `mix test.failure_injection` runs every tagged test, and
  `Lumia.FailureInjectionTest` checks that every listed case has one.
  """

  @specs [
    delivery: {"delivery.md", "## 12. Tests"},
    authorization: {"authorization.md", "## 11. Tests"},
    identity: {"identity.md", "## 12. Tests"}
  ]

  @docs Path.expand("../../../../docs/specs", __DIR__)

  @doc "Every case of the three lists, in order."
  def cases do
    for {spec, {file, heading}} <- @specs, {n, text} <- read(Path.join(@docs, file), heading) do
      {spec, n, text}
    end
  end

  @doc "The names of the specs the lists come from."
  def specs, do: Keyword.keys(@specs)

  @doc """
  The cases covered by loaded tests, as `%{{spec, n} => [test name]}`.
  Every test module must be loaded; `mix test` loads them all before
  running, a single file on its own does not.
  """
  def covered do
    for {module, _} <- :code.all_loaded(),
        function_exported?(module, :__ex_unit__, 0),
        %ExUnit.TestModule{tests: tests} = module.__ex_unit__(),
        %ExUnit.Test{name: name, tags: tags} <- tests,
        {spec, n} <- List.wrap(tags[:failure_injection]),
        reduce: %{} do
      acc -> Map.update(acc, {spec, n}, [name], &[name | &1])
    end
  end

  @doc "The test files that are loaded, for the check that the whole suite is."
  def loaded_test_files do
    for {module, _} <- :code.all_loaded(),
        function_exported?(module, :__ex_unit__, 0),
        %ExUnit.TestModule{file: file} = module.__ex_unit__(),
        uniq: true,
        do: file
  end

  defp read(path, heading) do
    text = File.read!(path)

    [_, rest] = String.split(text, heading <> "\n", parts: 2)
    [_, block | _] = String.split(rest, "```text\n", parts: 3)
    [block, _] = String.split(block, "```", parts: 2)

    block
    |> String.split("\n", trim: true)
    |> Enum.reduce([], fn
      " " <> _ = continuation, [current | done] ->
        [current <> " " <> String.trim(continuation) | done]

      first, acc ->
        [first | acc]
    end)
    |> Enum.reverse()
    |> Enum.with_index(1)
    |> Enum.map(fn {text, n} -> {n, text} end)
  end
end
