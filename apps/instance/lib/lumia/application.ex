defmodule Lumia.Application do
  # See https://elixir.hexdocs.pm/Application.html
  # for more information on OTP Applications
  @moduledoc false

  use Application

  @impl true
  def start(_type, _args) do
    children = [
      LumiaWeb.Telemetry,
      Lumia.Repo,
      # Loads instance settings (recovery generation) into persistent_term
      # before anything that sends them to clients starts.
      Lumia.Instance,
      Lumia.Authz.Cache,
      {Oban, Application.fetch_env!(:lumia, Oban)},
      {DNSCluster, query: Application.get_env(:lumia, :dns_cluster_query) || :ignore},
      {Phoenix.PubSub, name: Lumia.PubSub},
      # Start to serve requests, typically the last entry
      LumiaWeb.Endpoint
    ]

    # See https://elixir.hexdocs.pm/Supervisor.html
    # for other strategies and supported options
    opts = [strategy: :one_for_one, name: Lumia.Supervisor]
    Supervisor.start_link(children, opts)
  end

  # Tell Phoenix to update the endpoint configuration
  # whenever the application is updated.
  @impl true
  def config_change(changed, _new, removed) do
    LumiaWeb.Endpoint.config_change(changed, removed)
    :ok
  end
end
