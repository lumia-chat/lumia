import Config

# Credentials at boot; a platform without them routes to the fake
# provider, which only records. PORT defaults to 4100. Each credential is
# a file (APNS_KEY_FILE, FCM_SERVICE_ACCOUNT_FILE) or its contents inline
# (APNS_KEY, FCM_SERVICE_ACCOUNT), for hosts whose secrets are variables.
if port = System.get_env("PORT"), do: config(:lumia_relay, port: String.to_integer(port))

alias Lumia.Relay.Jwt

credential = fn inline, file ->
  case {System.get_env(inline), System.get_env(file)} do
    {nil, nil} -> nil
    {nil, path} -> File.read!(path)
    {value, _} -> value
  end
end

providers = %{}

providers =
  case credential.("APNS_KEY", "APNS_KEY_FILE") do
    nil ->
      providers

    pem ->
      key = pem |> Jwt.private_key_from_pem() |> Jwt.ec_scalar()

      config :lumia_relay, :apns,
        private_key: key,
        key_id: System.fetch_env!("APNS_KEY_ID"),
        team_id: System.fetch_env!("APNS_TEAM_ID"),
        topic: System.fetch_env!("APNS_TOPIC"),
        sandbox: System.get_env("APNS_SANDBOX") == "true"

      Map.put(providers, "apns", Lumia.Relay.Provider.Apns)
  end

providers =
  case credential.("FCM_SERVICE_ACCOUNT", "FCM_SERVICE_ACCOUNT_FILE") do
    nil ->
      providers

    json ->
      account = Jason.decode!(json)

      config :lumia_relay, :fcm,
        project_id: account["project_id"],
        client_email: account["client_email"],
        private_key: Jwt.private_key_from_pem(account["private_key"])

      Map.put(providers, "fcm", Lumia.Relay.Provider.Fcm)
  end

config :lumia_relay, :providers, providers
