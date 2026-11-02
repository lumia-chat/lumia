defmodule Lumia.Instance.Settings do
  @moduledoc "The single instance_settings row."

  use Lumia.Schema

  @primary_key {:singleton, :boolean, autogenerate: false}
  schema "instance_settings" do
    field :recovery_generation, :integer, default: 1
    # Web Push (RFC 8292): raw P-256 keys, generated at first boot.
    field :vapid_public_key, :binary, redact: true
    field :vapid_private_key, :binary, redact: true
    timestamps()
  end
end
