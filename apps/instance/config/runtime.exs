import Config

# config/runtime.exs is executed for all environments, including
# during releases. It is executed after compilation and before the
# system starts, so it is typically used to load production configuration
# and secrets from environment variables or elsewhere. Do not define
# any compile-time configuration in here, as it won't be applied.
# The block below contains prod specific runtime configuration.

# ## Using releases
#
# If you use `mix release`, you need to explicitly enable the server
# by passing the PHX_SERVER=true when you start it:
#
#     PHX_SERVER=true bin/lumia start
#
# Alternatively, you can use `mix phx.gen.release` to generate a `bin/server`
# script that automatically sets the env var above.
if System.get_env("PHX_SERVER") do
  config :lumia, LumiaWeb.Endpoint, server: true
end

config :lumia, LumiaWeb.Endpoint, http: [port: String.to_integer(System.get_env("PORT", "4000"))]

# The instance's own settings (docs/specs/deployment.md section 3), in
# every environment so a developer can set them too.
config :lumia, :instance,
  operator_name: System.get_env("LUMIA_OPERATOR_NAME", ""),
  operator_url: System.get_env("LUMIA_OPERATOR_URL", ""),
  terms_url: System.get_env("LUMIA_TERMS_URL", ""),
  privacy_url: System.get_env("LUMIA_PRIVACY_URL", ""),
  open_registration: System.get_env("LUMIA_OPEN_REGISTRATION", "true") == "true",
  # Magic-link sign-in needs a mailer; on in production only when one is
  # configured, and always in development and test, where the mailbox is local.
  magic_link:
    System.get_env("LUMIA_MAGIC_LINK", if(config_env() == :prod, do: "false", else: "true")) ==
      "true"

# OIDC as a client (architecture 6.3): one provider, set by the operator.
# LUMIA_OIDC_ISSUER is the issuer URL (discovery under it), LUMIA_OIDC_NAME
# what the sign-in button says.
if issuer = System.get_env("LUMIA_OIDC_ISSUER") do
  config :lumia, :oidc,
    issuer: issuer,
    client_id: System.fetch_env!("LUMIA_OIDC_CLIENT_ID"),
    client_secret: System.get_env("LUMIA_OIDC_CLIENT_SECRET", ""),
    name: System.get_env("LUMIA_OIDC_NAME", "Sign in with your organisation")
end

if origins = System.get_env("LUMIA_WEB_ORIGINS") do
  config :lumia, :identity,
    web_origins:
      origins |> String.split(",") |> Enum.map(&String.trim/1) |> Enum.reject(&(&1 == ""))
end

if path = System.get_env("LUMIA_MASTER_KEY_FILE") do
  config :lumia, :master_key_file, path
end

config :lumia, :metrics, System.get_env("LUMIA_METRICS") == "true"

# Push (architecture 15.5): the relay for APNs and FCM, and the contact
# the VAPID token names.
if relay = System.get_env("LUMIA_PUSH_RELAY_URL") do
  config :lumia, :push_relay_url, String.trim_trailing(relay, "/")
end

if contact = System.get_env("LUMIA_PUSH_CONTACT") do
  config :lumia, :push_contact, contact
end

if config_env() == :prod do
  database_url =
    System.get_env("DB_URL") ||
      raise """
      environment variable DB_URL is missing.
      For example: ecto://USER:PASS@HOST/DATABASE
      """

  maybe_ipv6 = if System.get_env("ECTO_IPV6") in ~w(true 1), do: [:inet6], else: []

  # TLS to Postgres (deployment.md section 3), verified with the host name
  # check either way: DB_SSL=true trusts the system store, for a database
  # with a publicly signed certificate; DB_CA_CERT (PEM) trusts that CA
  # instead, for a managed database signed by its own. Postgrex ignores an
  # `sslmode` query parameter in the URL, so one of these is the switch.
  ssl =
    case {System.get_env("DB_CA_CERT"), System.get_env("DB_SSL")} do
      {nil, enabled} ->
        enabled in ~w(true 1)

      {pem, _} ->
        case :public_key.pem_decode(pem) do
          [] ->
            raise "environment variable DB_CA_CERT holds no PEM certificate"

          entries ->
            [cacerts: for({:Certificate, der, :not_encrypted} <- entries, do: der)]
        end
    end

  config :lumia, Lumia.Repo,
    url: database_url,
    ssl: ssl,
    pool_size: String.to_integer(System.get_env("DB_POOL_SIZE") || "10"),
    # For machines with several cores, consider starting multiple pools of `pool_size`
    # pool_count: 4,
    socket_options: maybe_ipv6

  # The secret key base is used to sign/encrypt cookies and other secrets.
  # A default value is used in config/dev.exs and config/test.exs but you
  # want to use a different value for prod and you most likely don't want
  # to check this value into version control, so we use an environment
  # variable instead.
  secret_key_base =
    System.get_env("SECRET_KEY_BASE") ||
      raise """
      environment variable SECRET_KEY_BASE is missing.
      You can generate one by calling: mix phx.gen.secret
      """

  host = System.get_env("PHX_HOST") || "example.com"

  config :lumia, :dns_cluster_query, System.get_env("DNS_CLUSTER_QUERY")

  config :lumia, LumiaWeb.Endpoint,
    url: [host: host, port: 443, scheme: "https"],
    http: [
      # Enable IPv6 and bind on all interfaces.
      # Set it to  {0, 0, 0, 0, 0, 0, 0, 1} for local network only access.
      # See the documentation on https://bandit.hexdocs.pm/Bandit.html#t:options/0
      # for details about using IPv6 vs IPv4 and loopback vs public addresses.
      ip: {0, 0, 0, 0, 0, 0, 0, 0}
    ],
    secret_key_base: secret_key_base

  # ## SSL Support
  #
  # To get SSL working, you will need to add the `https` key
  # to your endpoint configuration:
  #
  #     config :lumia, LumiaWeb.Endpoint,
  #       https: [
  #         ...,
  #         port: 443,
  #         cipher_suite: :strong,
  #         keyfile: System.get_env("SOME_APP_SSL_KEY_PATH"),
  #         certfile: System.get_env("SOME_APP_SSL_CERT_PATH")
  #       ]
  #
  # The `cipher_suite` is set to `:strong` to support only the
  # latest and more secure SSL ciphers. This means old browsers
  # and clients may not be supported. You can set it to
  # `:compatible` for wider support.
  #
  # `:keyfile` and `:certfile` expect an absolute path to the key
  # and cert in disk or a relative path inside priv, for example
  # "priv/ssl/server.key". For all supported SSL configuration
  # options, see https://plug.hexdocs.pm/Plug.SSL.html#configure/1
  #
  # We also recommend setting `force_ssl` in your config/prod.exs,
  # ensuring no data is ever sent via http, always redirecting to https:
  #
  #     config :lumia, LumiaWeb.Endpoint,
  #       force_ssl: [hsts: true]
  #
  # Check `Plug.SSL` for all available options in `force_ssl`.

  # ## Configuring the mailer
  #
  # In production you need to configure the mailer to use a different adapter.
  # Here is an example configuration for Mailgun:
  #
  #     config :lumia, Lumia.Mailer,
  #       adapter: Swoosh.Adapters.Mailgun,
  #       api_key: System.get_env("MAILGUN_API_KEY"),
  #       domain: System.get_env("MAILGUN_DOMAIN")
  #
  # Most non-SMTP adapters require an API client. Swoosh supports Req, Hackney,
  # and Finch out-of-the-box. This configuration is typically done at
  # compile-time in your config/prod.exs:
  #
  #     config :swoosh, :api_client, Swoosh.ApiClient.Req
  #
  # See https://swoosh.hexdocs.pm/Swoosh.html#module-installation for details.
end
