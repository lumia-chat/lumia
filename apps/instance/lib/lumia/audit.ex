defmodule Lumia.Audit do
  @moduledoc """
  The audit log (docs/specs/authorization.md section 9). Rows are written in
  the same transaction as the change they describe. The viewing feature is
  deferred; the rows exist from the first release so it needs no backfill.
  """

  alias Lumia.Audit.Entry
  alias Lumia.Repo

  @doc """
  Records a change. Call inside the transaction that makes the change.

      Audit.record({:server, server_id}, actor_id, "role.update",
        target: {:role, role_id}, before: old, after: new)
  """
  def record({scope_kind, scope_id}, actor_account_id, action, opts \\ []) do
    {target_kind, target_id} =
      case Keyword.get(opts, :target) do
        {kind, id} -> {to_string(kind), id}
        nil -> {nil, nil}
      end

    Repo.insert(%Entry{
      scope_kind: to_string(scope_kind),
      scope_id: scope_id,
      actor_account_id: actor_account_id,
      action: action,
      target_kind: target_kind,
      target_id: target_id,
      before: Keyword.get(opts, :before),
      after: Keyword.get(opts, :after),
      override: Keyword.get(opts, :override, false),
      created_at: DateTime.utc_now()
    })
  end
end
