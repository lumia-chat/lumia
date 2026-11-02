defmodule Lumia.Workers.PurgeExpired do
  @moduledoc """
  Deletes expired identity rows (docs/specs/identity.md sections 6, 9, 10):
  used challenges past their expiry, unclaimed link bundles, and reauth
  tokens. Hourly from the Oban cron. Each table is small and indexed on
  `expires_at`, so one statement per table is enough.
  """

  use Oban.Worker, queue: :maintenance, max_attempts: 3

  import Ecto.Query

  alias Lumia.Identity.LinkBundle
  alias Lumia.Identity.ReauthToken
  alias Lumia.Identity.UsedChallenge
  alias Lumia.Repo

  @impl Oban.Worker
  def perform(%Oban.Job{}) do
    now = DateTime.utc_now()

    counts =
      for schema <- [UsedChallenge, LinkBundle, ReauthToken], into: %{} do
        {n, _} = Repo.delete_all(from r in schema, where: r.expires_at < ^now)
        {schema, n}
      end

    {:ok, counts}
  end
end
