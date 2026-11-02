defmodule Lumia.Workers.ExpireGuests do
  @moduledoc """
  Removes guest memberships whose `expires_at` has passed (architecture
  6.2). Each removal goes through `Lumia.Servers.remove_member/4` with the
  reason `guest_expired`, so the member's account log gets
  `MembershipRemoved` and the channels they could view get
  `ChannelRemoved`, exactly as a kick would. Hourly from the Oban cron.
  """

  use Oban.Worker, queue: :maintenance, max_attempts: 3

  import Ecto.Query

  alias Lumia.Repo
  alias Lumia.Servers
  alias Lumia.Servers.Member

  @impl Oban.Worker
  def perform(%Oban.Job{}) do
    {:ok, expire(DateTime.utc_now())}
  end

  @doc "Removes every guest expired at `now`. Returns how many were removed."
  def expire(%DateTime{} = now) do
    from(m in Member,
      where: m.kind == "guest" and not is_nil(m.expires_at) and m.expires_at < ^now,
      select: {m.server_id, m.account_id}
    )
    |> Repo.all()
    |> Enum.count(fn {server_id, account_id} ->
      Servers.remove_member(server_id, account_id, nil, "guest_expired") == :ok
    end)
  end
end
