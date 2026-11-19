defmodule Lumia.Workers.RolesUpdated do
  @moduledoc """
  Tells every member of a server that its roles changed: appends
  `RolesUpdated`, the server's roles as they now are, to each member's
  account log (architecture 7.5, authorization 6.4), so every client can
  draw a member's tag and knows what its own roles let it offer.

  Scheduled when a change commits. Edits made while a job waits are one
  job, which carries the roles as they are when it runs; an edit made
  while one runs schedules another.
  """

  use Oban.Worker,
    queue: :default,
    max_attempts: 5,
    unique: [period: 60, keys: [:server_id], states: [:available, :scheduled, :retryable]]

  alias Lumia.Proto
  alias Lumia.Servers
  alias Lumia.Servers.Fanout
  alias Lumia.V1

  @delay_seconds 1

  @doc "Schedules the job for a server. Safe to call inside a transaction."
  def schedule(server_id) do
    Oban.insert(new(%{"server_id" => server_id}, schedule_in: @delay_seconds))
  end

  @impl Oban.Worker
  def perform(%Oban.Job{args: %{"server_id" => server_id}}) do
    case Servers.roles(server_id) do
      # The server is gone, and its members with it.
      [] ->
        :ok

      roles ->
        Fanout.to_members(server_id, [
          {:roles_updated,
           %V1.RolesUpdated{
             server_id: Proto.uuid(server_id),
             roles: Enum.map(roles, &Proto.role/1)
           }}
        ])
    end
  end
end
