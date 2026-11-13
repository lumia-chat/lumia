defmodule Lumia.Workers.ReadPush do
  @moduledoc """
  Wakes an account's devices that are not connected after it read
  something on another (`Lumia.Push.read_elsewhere/1`), off the request
  path. One job for a burst of reads by the same account.
  """

  use Oban.Worker,
    queue: :default,
    max_attempts: 3,
    unique: [period: 5, keys: [:account_id], states: [:available, :scheduled]]

  @impl Oban.Worker
  def perform(%Oban.Job{args: %{"account_id" => account_id}}) do
    Lumia.Push.deliver_read(account_id)
    :ok
  end
end
