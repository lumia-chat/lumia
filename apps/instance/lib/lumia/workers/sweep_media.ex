defmodule Lumia.Workers.SweepMedia do
  @moduledoc """
  Hourly from the Oban cron (specs/media.md 4.4 and 6.4): attachment blobs
  no message named within a day, those of messages deleted past their
  window, link cards past their hour that no message names, with their
  pictures, and account and server pictures nothing names any more.
  """

  use Oban.Worker, queue: :maintenance, max_attempts: 3

  @impl Oban.Worker
  def perform(%Oban.Job{}) do
    {:ok,
     %{
       uploads: Lumia.Uploads.sweep(),
       cards: Lumia.Cards.sweep(),
       pictures: Lumia.Blobs.sweep_pictures()
     }}
  end
end
