defmodule Lumia.Relay.MixProject do
  use Mix.Project

  def project do
    [
      app: :lumia_relay,
      version: "0.7.0",
      elixir: "~> 1.20",
      start_permanent: Mix.env() == :prod,
      deps: deps(),
      aliases: [precommit: ["compile --warnings-as-errors", "format", "credo --strict", "test"]],
      releases: [lumia_relay: [include_executables_for: [:unix]]]
    ]
  end

  def cli do
    [preferred_envs: [precommit: :test]]
  end

  # Run "mix help compile.app" to learn about applications.
  def application do
    [
      extra_applications: [:logger, :crypto, :public_key],
      mod: {Lumia.Relay.Application, []}
    ]
  end

  # Run "mix help deps" to learn about dependencies.
  defp deps do
    [
      {:bandit, "~> 1.7"},
      {:plug, "~> 1.18"},
      {:req, "~> 0.5"},
      {:jason, "~> 1.4"},
      {:credo, "~> 1.7", only: [:dev, :test], runtime: false}
    ]
  end
end
