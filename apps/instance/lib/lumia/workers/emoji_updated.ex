defmodule Lumia.Workers.EmojiUpdated do
  @moduledoc """
  Tells every member of a server that its custom emoji changed: appends
  `EmojiUpdated`, the server's live emoji as they now are, to each
  member's account log (specs/emoji.md 8), as `Lumia.Workers.RolesUpdated`
  tells of its roles: one job for a burst of changes, the emoji read once
  the changes in flight have committed, a server's jobs one at a time.
  """

  use Oban.Worker,
    queue: :default,
    max_attempts: 5,
    unique: [period: 60, keys: [:server_id], states: [:available, :scheduled, :retryable]]

  alias Lumia.CustomEmoji
  alias Lumia.Proto
  alias Lumia.Servers.Fanout
  alias Lumia.V1

  @delay_seconds 1

  @doc "Schedules the job for a server. Safe to call inside a transaction."
  def schedule(server_id) do
    Oban.insert(new(%{"server_id" => server_id}, schedule_in: @delay_seconds))
  end

  @impl Oban.Worker
  def perform(%Oban.Job{args: %{"server_id" => server_id}}) do
    Fanout.one_at_a_time(server_id, fn -> tell(server_id) end)
  end

  defp tell(server_id) do
    case Fanout.settled(server_id, fn -> CustomEmoji.list(server_id) end) do
      # The server is gone, and its members with it.
      nil ->
        :ok

      emoji ->
        Fanout.to_members(server_id, [
          {:emoji_updated,
           %V1.EmojiUpdated{
             server_id: Proto.uuid(server_id),
             emoji: Enum.map(emoji, &Proto.custom_emoji/1)
           }}
        ])
    end
  end
end
