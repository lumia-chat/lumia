defmodule LumiaWeb.InstanceController do
  @moduledoc "`GET /api/v1/instance`: the trust card (architecture 6.1). Public."

  use Phoenix.Controller, formats: []

  alias Lumia.Instance
  alias Lumia.V1
  alias LumiaWeb.ProtoConn

  def show(conn, _params) do
    config = Application.get_env(:lumia, :instance, [])

    info = %V1.InstanceInfo{
      host: LumiaWeb.Endpoint.host(),
      operator_name: Keyword.get(config, :operator_name, ""),
      operator_url: Keyword.get(config, :operator_url, ""),
      terms_url: Keyword.get(config, :terms_url, ""),
      privacy_url: Keyword.get(config, :privacy_url, ""),
      software_version: Application.spec(:lumia, :vsn) |> to_string(),
      identity_proof_sign_in_enabled: Keyword.get(config, :identity_proof_sign_in, true),
      identity_proof_create_enabled: Keyword.get(config, :identity_proof_create, true),
      open_registration: Keyword.get(config, :open_registration, true),
      web_push_public_key:
        case Lumia.Instance.settings().vapid_public_key do
          nil -> ""
          key -> Base.url_encode64(key, padding: false)
        end,
      turn_configured: false,
      push_relay_enabled: false,
      reactions:
        Enum.map(reactions(), fn {key, label} ->
          %V1.ReactionDefinition{key: key, label: label}
        end),
      recovery_generation: Instance.recovery_generation()
    }

    ProtoConn.render(conn, info)
  end

  # The built-in reaction registry (architecture 6.6).
  defp reactions do
    [{"thumbs_up", "Thumbs up"}, {"check", "Check"}, {"heart", "Heart"}, {"sparkle", "Sparkle"}]
  end
end
