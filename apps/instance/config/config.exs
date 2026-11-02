# This file is responsible for configuring your application
# and its dependencies with the aid of the Config module.
#
# This configuration file is loaded before any dependency and
# is restricted to this project.

# General application configuration
import Config

config :lumia,
  ecto_repos: [Lumia.Repo],
  generators: [timestamp_type: :utc_datetime_usec, binary_id: true]

# Every table gets UUID keys and microsecond UTC timestamps. IDs are minted
# app-side as monotonic UUIDv7 (see Lumia.Schema); migrations declare the
# column type only and never a database default.
config :lumia, Lumia.Repo,
  migration_primary_key: [type: :binary_id],
  migration_foreign_key: [type: :binary_id],
  migration_timestamps: [type: :utc_datetime_usec]

# Background work (architecture section 3): retention, purges, viewer diffs,
# webhook retries. Postgres-backed; no extra service.
config :lumia, Oban,
  repo: Lumia.Repo,
  queues: [default: 10, maintenance: 2],
  plugins: [
    {Oban.Plugins.Pruner, max_age: 60 * 60 * 24 * 7},
    {Oban.Plugins.Cron,
     crontab: [
       {"0 3 * * *", Lumia.Workers.PurgeCommands},
       {"30 3 * * *", Lumia.Workers.Retention},
       {"15 * * * *", Lumia.Workers.PurgeExpired},
       {"45 * * * *", Lumia.Workers.ExpireGuests}
     ]}
  ]

# Configure the endpoint
config :lumia, LumiaWeb.Endpoint,
  url: [host: "localhost"],
  adapter: Bandit.PhoenixAdapter,
  render_errors: [
    formats: [json: LumiaWeb.ErrorJSON],
    layout: false
  ],
  pubsub_server: Lumia.PubSub,
  live_view: [signing_salt: "A3i3OdaE"]

# Configure the mailer
#
# By default it uses the "Local" adapter which stores the emails
# locally. You can see the emails in your browser, at "/dev/mailbox".
#
# For production it's recommended to configure a different adapter
# at the `config/runtime.exs`.
config :lumia, Lumia.Mailer, adapter: Swoosh.Adapters.Local

# Configure Elixir's Logger
config :logger, :default_formatter,
  format: "$time $metadata[$level] $message\n",
  metadata: [:request_id]

# Use Jason for JSON parsing in Phoenix
config :phoenix, :json_library, Jason

# The API's binary encoding (docs/protocol.md section 2).
config :mime, :types, %{"application/x-protobuf" => ["x-protobuf"]}

# Import environment specific config. This must remain at the bottom
# of this file so it overrides the configuration defined above.
import_config "#{config_env()}.exs"
