defmodule Lumia.Workers.Push do
  @moduledoc "Carries out one push decision (`Lumia.Push.after_commit/2`) off the request path."

  use Oban.Worker, queue: :default, max_attempts: 3

  alias Lumia.Events

  @impl Oban.Worker
  def perform(%Oban.Job{args: %{"channel_id" => channel_id, "seq" => seq}}) do
    case Events.events_after(channel_id, seq - 1, 1) do
      [%Lumia.V1.Event{seq: ^seq} = event] ->
        Lumia.Push.deliver(channel_id, event)
        :ok

      _ ->
        :ok
    end
  end
end
