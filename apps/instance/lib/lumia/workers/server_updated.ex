defmodule Lumia.Workers.ServerUpdated do
  @moduledoc """
  Tells every member of a server that its name, picture or retention
  changed: appends `ServerUpdated`, the server as it now is, to each
  member's account log (architecture 6.7), a batch of members to a
  transaction, so an admin's request never waits on a large server.

  Scheduled when a change commits. Edits made while a job waits are one
  job, which carries the server as it is when it runs; an edit made while
  one runs schedules another.
  """

  use Oban.Worker,
    queue: :default,
    max_attempts: 5,
    unique: [period: 60, keys: [:server_id], states: [:available, :scheduled, :retryable]]

  import Ecto.Query

  alias Lumia.Accounts
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.Servers.Member
  alias Lumia.Servers.Server
  alias Lumia.Tx
  alias Lumia.V1

  @batch 500
  @delay_seconds 1

  @doc "Schedules the job for a server. Safe to call inside a transaction."
  def schedule(server_id) do
    Oban.insert(new(%{"server_id" => server_id}, schedule_in: @delay_seconds))
  end

  @impl Oban.Worker
  def perform(%Oban.Job{args: %{"server_id" => server_id}}) do
    case Repo.get(Server, server_id) do
      nil ->
        :ok

      server ->
        event = {:server_updated, %V1.ServerUpdated{server: Proto.server(server)}}

        from(m in Member,
          where: m.server_id == ^server_id,
          order_by: m.account_id,
          select: m.account_id
        )
        |> Repo.all()
        |> Enum.chunk_every(@batch)
        |> Enum.each(&tell(&1, event))
    end
  end

  # One batch of members, in one transaction.
  defp tell(accounts, event) do
    {:ok, _} =
      Tx.transact(fn ->
        Enum.each(accounts, &({:ok, _} = Accounts.append_event(&1, event)))
        {:ok, :ok}
      end)
  end
end
