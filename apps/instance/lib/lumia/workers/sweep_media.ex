defmodule Lumia.Workers.SweepMedia do
  @moduledoc """
  Hourly from the Oban cron (specs/media.md 4.4 and 6.4): attachment blobs
  no message named within a day, those of messages deleted past their
  window, and link cards past their hour that no message names, with their
  pictures.
  """

  use Oban.Worker, queue: :maintenance, max_attempts: 3

  @impl Oban.Worker
  def perform(%Oban.Job{}) do
    {:ok, %{uploads: Lumia.Uploads.sweep(), cards: Lumia.Cards.sweep()}}
  end
end
