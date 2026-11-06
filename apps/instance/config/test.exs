import Config

# Configure your database
#
# The MIX_TEST_PARTITION environment variable can be used
# to provide built-in test partitioning in CI environment.
# Run `mix help test` for more information.
if url = System.get_env("DB_URL") do
  config :lumia, Lumia.Repo, url: url
else
  config :lumia, Lumia.Repo,
    username: "postgres",
    password: "postgres",
    hostname: "localhost",
    database: "lumia_test#{System.get_env("MIX_TEST_PARTITION")}"
end

config :lumia, Lumia.Repo,
  pool: Ecto.Adapters.SQL.Sandbox,
  pool_size: System.schedulers_online() * 2

# Oban runs inline in tests; workers are exercised directly.
config :lumia, Oban, testing: :manual
config :lumia, :push_loopback_endpoints, true

# We don't run a server during test. If one is required,
# you can enable the server option below.
config :lumia, LumiaWeb.Endpoint,
  http: [ip: {127, 0, 0, 1}, port: 4002],
  secret_key_base: "dNCbpZSB0RDU9Y8jHb+FOgVgEGt3XXKkSbL15veVNyEHo3I+Hjg9vN6tDBi2XMuF",
  server: false

# In test we don't send emails
config :lumia, Lumia.Mailer, adapter: Swoosh.Adapters.Test

# Disable swoosh api client as it is only required for production adapters
config :swoosh, :api_client, false

# Print only warnings and errors during test
config :logger, level: :warning

# Initialize plugs at runtime for faster test compilation
config :phoenix, :plug_init_mode, :runtime

# Sort query params output of verified routes for robust url comparisons
config :phoenix,
  sort_verified_routes_query_params: true

# A trusted web origin for the CORS and web-device tests.
config :lumia, :identity, web_origins: ["http://localhost:5173"]

# Pushes go to the test that asked for them, never to a push service.
config :lumia, :push_sender, Lumia.Push.Sender.Test

# The mailbox is local here, so magic links can be tried.
config :lumia, :instance, magic_link: true

# The gateway's reconciler ticks by hand in the tests (a timed tick would
# race their checks and runs outside the SQL sandbox).
config :lumia, reconciler_tick_ms: :timer.minutes(10)
