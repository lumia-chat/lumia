defmodule Lumia.Servers.Fanout do
  @moduledoc """
  Tells every member of a server something through its account log
  (architecture 6.7 and 7.5): the jobs that run after a change commits
  append here, a batch of members to a transaction, so no request waits on
  a large server.
  """

  import Ecto.Query

  alias Lumia.Accounts
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.Servers
  alias Lumia.Servers.Member
  alias Lumia.Servers.Server
  alias Lumia.Tx
  alias Lumia.V1

  @batch 500

  @doc """
  Appends each of `events`, `Lumia.V1.AccountEvent` payload tuples, in
  order to the account log of every member of the server. A guest is told
  of only the members it sees (authorization 6.4): a `MembersUpdated` it
  is appended names those alone, and is not appended when it names none
  of them.
  """
  def to_members(_server_id, []), do: :ok

  def to_members(server_id, events) do
    {guests, members} =
      from(m in Member,
        where: m.server_id == ^server_id,
        order_by: m.account_id,
        select: {m.account_id, m.kind}
      )
      |> Repo.all()
      |> Enum.split_with(fn {_, kind} -> kind == "guest" end)

    members
    |> Enum.map(&elem(&1, 0))
    |> Enum.chunk_every(@batch)
    |> Enum.each(fn batch -> tell(batch, fn _ -> events end) end)

    guest_ids = Enum.map(guests, &elem(&1, 0))
    seen = Servers.seen_by_guests(server_id, guest_ids)

    guest_ids
    |> Enum.chunk_every(@batch)
    |> Enum.each(fn batch -> tell(batch, &for_guest(events, Map.fetch!(seen, &1))) end)
  end

  # What a guest is told of `events`: the members it sees, and nothing of
  # the rest.
  defp for_guest(events, seen) do
    Enum.flat_map(events, fn
      {:members_updated, %V1.MembersUpdated{members: members} = event} ->
        case Enum.filter(members, &MapSet.member?(seen, Proto.from_uuid(&1.account_id))) do
          [] -> []
          kept -> [{:members_updated, %{event | members: kept}}]
        end

      event ->
        [event]
    end)
  end

  @doc """
  Runs `fun`, a job's reading and telling for a server, while no other
  job does so for that server on any node, and returns what it returns. A
  large server's telling can outlast the second between two jobs; were
  they to overlap, a member could be told what the later one read and
  then what the earlier one did, and keep the older.

  The connection is this job's until `fun` returns, and the lock is the
  session's: a job that dies gives up both.
  """
  def one_at_a_time(server_id, fun) when is_function(fun, 0) do
    key = :erlang.phash2({__MODULE__, server_id})

    Repo.checkout(
      fn ->
        Repo.query!("SELECT pg_advisory_lock($1)", [key], timeout: :infinity)

        try do
          fun.()
        after
          Repo.query!("SELECT pg_advisory_unlock($1)", [key])
        end
      end,
      timeout: :infinity
    )
  end

  @doc """
  What `read` answers once every change to the server now in flight has
  committed; nil, and `read` not run, when the server is gone.

  A change that is told to the members holds the server's row from before
  it schedules its job until it commits (`Lumia.Servers`). A job is made
  one with a change only while it has not begun, so it begins after that
  change took the row, waits here for the commit, and reads the change.
  Without the wait, a job beginning in that moment read what was there
  before, and nothing told the members of the change until the next one.
  """
  def settled(server_id, read) when is_function(read, 0) do
    {:ok, value} =
      Tx.transact(fn ->
        held =
          Repo.one(from s in Server, where: s.id == ^server_id, select: s.id, lock: "FOR SHARE")

        {:ok, held && read.()}
      end)

    value
  end

  # One batch of members, in one transaction, each told what `events_for`
  # answers for it.
  defp tell(accounts, events_for) do
    {:ok, _} =
      Tx.transact(fn ->
        for account <- accounts, event <- events_for.(account) do
          {:ok, _} = Accounts.append_event(account, event)
        end

        {:ok, :ok}
      end)

    :ok
  end
end
