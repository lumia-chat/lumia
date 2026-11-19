defmodule Lumia.Servers.Fanout do
  @moduledoc """
  Tells every member of a server something through its account log
  (architecture 6.7 and 7.5): the jobs that run after a change commits
  append here, a batch of members to a transaction, so no request waits on
  a large server.
  """

  import Ecto.Query

  alias Lumia.Accounts
  alias Lumia.Repo
  alias Lumia.Servers.Member
  alias Lumia.Tx

  @batch 500

  @doc """
  Appends each of `events`, `Lumia.V1.AccountEvent` payload tuples, in
  order to the account log of every member of the server.
  """
  def to_members(_server_id, []), do: :ok

  def to_members(server_id, events) do
    from(m in Member,
      where: m.server_id == ^server_id,
      order_by: m.account_id,
      select: m.account_id
    )
    |> Repo.all()
    |> Enum.chunk_every(@batch)
    |> Enum.each(&tell(&1, events))
  end

  # One batch of members, in one transaction.
  defp tell(accounts, events) do
    {:ok, _} =
      Tx.transact(fn ->
        for account <- accounts, event <- events do
          {:ok, _} = Accounts.append_event(account, event)
        end

        {:ok, :ok}
      end)

    :ok
  end
end
