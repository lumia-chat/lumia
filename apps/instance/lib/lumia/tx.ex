defmodule Lumia.Tx do
  @moduledoc """
  Transactions whose side effects wait for the commit.

  The delivery spec (section 4) and the hard rules say an event is
  broadcast after its transaction commits, never inside it. Ecto flattens
  a nested `Repo.transact/1` into the outer one, so a context that appends
  an event inside its own transaction would otherwise broadcast while the
  outer transaction is still open: a session could push an event whose
  row is not yet visible, or one whose transaction then rolls back, after
  which the same `seq` is reused and the client drops the real event as a
  duplicate.

  `transact/2` is `Repo.transact/2` plus a depth counter in the process
  dictionary. `after_commit/1` runs its function at once outside any
  `transact/2`, and otherwise queues it until the outermost `transact/2`
  returns `{:ok, _}`. On `{:error, _}`, a rollback or a raise, the queue
  is discarded. Contexts open transactions through this module and
  broadcast through `after_commit/1`; nothing else in `lib/` calls
  `Repo.transact/1` directly.
  """

  alias Lumia.Repo

  require Logger

  @depth {__MODULE__, :depth}
  @queue {__MODULE__, :queue}

  @doc """
  `Repo.transact/2` with deferred side effects. Nested calls join the
  outer transaction as Ecto does; only the outermost call releases the
  queue, and only on `{:ok, _}`.
  """
  def transact(fun, opts \\ []) when is_function(fun, 0) do
    depth = Process.get(@depth, 0)
    Process.put(@depth, depth + 1)

    try do
      result = Repo.transact(fun, opts)
      if depth == 0, do: settle(result)
      result
    after
      if depth == 0 do
        Process.delete(@depth)
        Process.delete(@queue)
      else
        Process.put(@depth, depth)
      end
    end
  end

  @doc """
  Runs `fun` now when no `transact/2` is open in this process; otherwise
  after the outermost one commits. Returns `:ok` either way.
  """
  def after_commit(fun) when is_function(fun, 0) do
    if Process.get(@depth, 0) > 0 do
      Process.put(@queue, [fun | Process.get(@queue, [])])
    else
      fun.()
    end

    :ok
  end

  @doc "Whether a `transact/2` is open in this process. For tests."
  def open?, do: Process.get(@depth, 0) > 0

  defp settle({:ok, _}) do
    @queue
    |> Process.get([])
    |> Enum.reverse()
    |> Enum.each(&run/1)
  end

  defp settle(_not_committed), do: :ok

  # A broadcast that fails must not undo a commit that already happened;
  # the reconciler delivers what the broadcast did not.
  defp run(fun) do
    fun.()
  rescue
    e -> Logger.error("after-commit hook failed: " <> Exception.message(e))
  end
end
