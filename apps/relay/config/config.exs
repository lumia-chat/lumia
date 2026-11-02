import Config

config :lumia_relay, port: 4100

import_config "#{config_env()}.exs"
