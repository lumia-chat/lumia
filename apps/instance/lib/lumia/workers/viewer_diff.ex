defmodule Lumia.Workers.ViewerDiff do
  @moduledoc """
  Recomputes who can view each channel in a server and appends
  `channel_added` or `channel_removed` to the account logs of members who
  gained or lost view (docs/specs/authorization.md section 6.3).

  Debounced per server: a burst of admin edits runs once. Discovery is
  therefore eventually consistent in the background; delivery is stopped
  immediately by `acl_version`.
  """

  use Oban.Worker, queue: :default, max_attempts: 5, unique: [period: 30, keys: [:server_id]]

  import Ecto.Query

  alias Lumia.Accounts
  alias Lumia.Events.Channel
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.Servers
  alias Lumia.Servers.ChannelViewer
  alias Lumia.Tx

  @debounce_seconds 30

  @doc "Schedules the job for a server, debounced. Safe to call inside a transaction."
  def schedule(server_id) do
    Oban.insert(new(%{"server_id" => server_id}, schedule_in: @debounce_seconds))
  end

  @impl Oban.Worker
  def perform(%Oban.Job{args: %{"server_id" => server_id}}) do
    channel_ids =
      Repo.all(
        from c in Channel, where: c.server_id == ^server_id and is_nil(c.deleted_at), select: c.id
      )

    Enum.each(channel_ids, &diff_channel/1)
    :ok
  end

  @doc "Diffs one channel now. Used by tests and by channel creation."
  def diff_channel(channel_id) do
    current = MapSet.new(Servers.viewers(channel_id))

    previous =
      MapSet.new(
        Repo.all(
          from v in ChannelViewer, where: v.channel_id == ^channel_id, select: v.account_id
        )
      )

    added = MapSet.difference(current, previous)
    removed = MapSet.difference(previous, current)

    Tx.transact(fn ->
      for account_id <- added do
        {:ok, _} = Repo.insert(%ChannelViewer{channel_id: channel_id, account_id: account_id})

        {:ok, _} =
          Accounts.append_event(
            account_id,
            {:channel_added,
             %Lumia.V1.ChannelAdded{channel: Proto.channel(Repo.get!(Channel, channel_id))}}
          )
      end

      for account_id <- removed do
        Repo.delete_all(
          from v in ChannelViewer,
            where: v.channel_id == ^channel_id and v.account_id == ^account_id
        )

        {:ok, _} =
          Accounts.append_event(
            account_id,
            {:channel_removed, %Lumia.V1.ChannelRemoved{channel_id: Proto.uuid(channel_id)}}
          )
      end

      {:ok, %{added: MapSet.to_list(added), removed: MapSet.to_list(removed)}}
    end)
    |> elem(1)
  end
end
