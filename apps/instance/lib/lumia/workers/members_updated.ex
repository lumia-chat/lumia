defmodule Lumia.Workers.MembersUpdated do
  @moduledoc """
  Tells every member of a server which members' roles or titles changed:
  appends `MembersUpdated`, those members as they now are, to each
  member's account log (architecture 7.5, authorization 6.4), so a tag
  follows a change on every client.

  The members to tell of are the server's `member_announcements` rows,
  written with each change. The job reads them, tells, and deletes the
  rows it read: a change made while it runs has a row of its own and
  schedules a job of its own. Run twice it tells twice, which costs
  nothing, since each event says how a member is and not what changed.
  """

  use Oban.Worker,
    queue: :default,
    max_attempts: 5,
    unique: [period: 60, keys: [:server_id], states: [:available, :scheduled, :retryable]]

  import Ecto.Query

  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.Servers.Fanout
  alias Lumia.Servers.Member
  alias Lumia.Servers.MemberAnnouncement
  alias Lumia.Servers.MemberRole
  alias Lumia.V1

  @delay_seconds 1
  @members_per_event 200

  @doc "Schedules the job for a server. Safe to call inside a transaction."
  def schedule(server_id) do
    Oban.insert(new(%{"server_id" => server_id}, schedule_in: @delay_seconds))
  end

  @impl Oban.Worker
  def perform(%Oban.Job{args: %{"server_id" => server_id}}) do
    rows =
      Repo.all(
        from a in MemberAnnouncement,
          where: a.server_id == ^server_id,
          select: {a.id, a.account_id}
      )

    account_ids = rows |> Enum.map(&elem(&1, 1)) |> Enum.uniq()
    Fanout.to_members(server_id, events(server_id, account_ids))

    ids = Enum.map(rows, &elem(&1, 0))
    Repo.delete_all(from a in MemberAnnouncement, where: a.id in ^ids)
    :ok
  end

  # The members named, as they are now; one who has left is left out.
  defp events(server_id, account_ids) do
    roles =
      from(mr in MemberRole,
        where: mr.server_id == ^server_id and mr.account_id in ^account_ids,
        select: {mr.account_id, mr.role_id}
      )
      |> Repo.all()
      |> Enum.group_by(&elem(&1, 0), &elem(&1, 1))

    from(m in Member,
      where: m.server_id == ^server_id and m.account_id in ^account_ids,
      order_by: m.account_id
    )
    |> Repo.all()
    |> Enum.map(&Proto.member(&1, Map.get(roles, &1.account_id, [])))
    |> Enum.chunk_every(@members_per_event)
    |> Enum.map(fn members ->
      {:members_updated, %V1.MembersUpdated{server_id: Proto.uuid(server_id), members: members}}
    end)
  end
end
