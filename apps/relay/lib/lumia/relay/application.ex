defmodule Lumia.Relay.Application do
  @moduledoc false

  use Application

  @impl true
  def start(_type, _args) do
    port = Application.get_env(:lumia_relay, :port, 4100)

    children = [
      {Bandit, plug: Lumia.Relay.Router, port: port}
    ]

    Supervisor.start_link(children, strategy: :one_for_one, name: Lumia.Relay.Supervisor)
  end
end
