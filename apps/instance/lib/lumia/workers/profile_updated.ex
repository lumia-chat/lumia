defmodule Lumia.Workers.ProfileUpdated do
  @moduledoc """
  Tells everyone who can see an account that its name or picture changed:
  appends `ProfileUpdated`, the account as others see it, to the account
  log of every account that shares a server or a personal conversation
  with it (architecture 6.2), a batch to a transaction, so a rename never
  waits on the size of the servers it is in. The account's own log has the
  event already, written with the change.

  Scheduled when a change commits. Changes made while a job waits are one
  job, which carries the account as it is when it runs; a change made while
  one runs schedules another.
  """

  use Oban.Worker,
    queue: :default,
    max_attempts: 5,
    unique: [period: 60, keys: [:account_id], states: [:available, :scheduled, :retryable]]

  import Ecto.Query

  alias Lumia.Accounts
  alias Lumia.Accounts.Account
  alias Lumia.Dms.Participant
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.Servers.Member
  alias Lumia.Tx
  alias Lumia.V1

  @batch 500
  @delay_seconds 1

  @doc "Schedules the job for an account. Safe to call inside a transaction."
  def schedule(account_id) do
    Oban.insert(new(%{"account_id" => account_id}, schedule_in: @delay_seconds))
  end

  @impl Oban.Worker
  def perform(%Oban.Job{args: %{"account_id" => account_id}}) do
    case Repo.get(Account, account_id) do
      nil ->
        :ok

      account ->
        event = {:profile_updated, %V1.ProfileUpdated{account: Proto.account(account)}}

        account_id
        |> others()
        |> Enum.chunk_every(@batch)
        |> Enum.each(&tell(&1, event))
    end
  end

  @doc "The accounts that share a server or a personal conversation with `account_id`, itself not among them."
  def others(account_id) do
    servers = from(m in Member, where: m.account_id == ^account_id, select: m.server_id)

    conversations =
      from(p in Participant, where: p.account_id == ^account_id, select: p.channel_id)

    in_servers =
      from(m in Member,
        where: m.server_id in subquery(servers) and m.account_id != ^account_id,
        select: %{account_id: m.account_id}
      )

    in_conversations =
      from(p in Participant,
        where: p.channel_id in subquery(conversations) and p.account_id != ^account_id,
        select: %{account_id: p.account_id}
      )

    from(a in subquery(union(in_servers, ^in_conversations)),
      distinct: true,
      order_by: a.account_id,
      select: a.account_id
    )
    |> Repo.all()
  end

  # One batch of accounts, in one transaction.
  defp tell(accounts, event) do
    {:ok, _} =
      Tx.transact(fn ->
        Enum.each(accounts, &({:ok, _} = Accounts.append_event(&1, event)))
        {:ok, :ok}
      end)
  end
end
