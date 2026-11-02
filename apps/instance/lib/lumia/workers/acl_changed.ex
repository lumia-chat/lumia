defmodule Lumia.Workers.AclChanged do
  @moduledoc """
  Appends an `acl_changed` event to every channel in a server after a
  server-wide `acl_version` bump (docs/specs/authorization.md section 6.1).

  The bump itself is synchronous in the admin's transaction; appending one
  event per channel is done here in batches so a role edit on a
  500-channel server does not write 500 event rows inside that
  transaction. Delivery is already gated by the bumped version; this event
  only makes quiet channels' sessions re-resolve promptly.
  """

  use Oban.Worker, queue: :default, max_attempts: 5, unique: [period: 5, keys: [:server_id]]

  import Ecto.Query

  alias Lumia.Events
  alias Lumia.Events.Channel
  alias Lumia.Repo

  @batch 100

  @doc "Schedules the job for a server. Safe to call inside a transaction."
  @spec schedule(Ecto.UUID.t()) :: {:ok, Oban.Job.t()} | {:error, term()}
  def schedule(server_id), do: Oban.insert(new(%{"server_id" => server_id}))

  @impl Oban.Worker
  def perform(%Oban.Job{args: %{"server_id" => server_id}}) do
    from(c in Channel,
      where: c.server_id == ^server_id and is_nil(c.deleted_at),
      select: {c.id, c.acl_version}
    )
    |> Repo.all()
    |> Enum.chunk_every(@batch)
    |> Enum.each(fn batch ->
      Enum.each(batch, fn {channel_id, acl_version} ->
        {:ok, _} =
          Events.append_system(channel_id, "acl_changed", %{"acl_version" => acl_version})
      end)
    end)

    :ok
  end
end
