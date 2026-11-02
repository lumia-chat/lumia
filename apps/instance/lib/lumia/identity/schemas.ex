defmodule Lumia.Identity.HeldRoster do
  @moduledoc "The newest verified roster this instance holds for an identity."

  use Lumia.Schema

  @primary_key false
  schema "identity_rosters" do
    field :identity_id, :binary, primary_key: true
    field :version, :integer
    field :root_public_key, :binary
    field :roster, :binary
    timestamps()
  end
end

defmodule Lumia.Identity.UsedChallenge do
  @moduledoc "A consumed challenge nonce; a duplicate insert is a replay."

  use Lumia.Schema

  @primary_key false
  schema "used_challenges" do
    field :nonce, :binary, primary_key: true
    field :expires_at, :utc_datetime_usec
  end
end

defmodule Lumia.Identity.LinkBundle do
  @moduledoc "An encrypted bundle for a device being added, claimed by a short code."

  use Lumia.Schema

  @primary_key false
  schema "link_bundles" do
    field :code, :string, primary_key: true
    field :device_id, Ecto.UUID
    field :ciphertext, :binary
    field :sender_ephemeral_public_key, :binary
    field :nonce, :binary
    field :attempts, :integer, default: 0
    field :expires_at, :utc_datetime_usec
    field :created_at, :utc_datetime_usec
  end
end

defmodule Lumia.Identity.Backup do
  @moduledoc "The encrypted identity backup; the host cannot read it."

  use Lumia.Schema

  @primary_key false
  schema "identity_backups" do
    field :identity_id, :binary, primary_key: true
    field :version, :integer
    field :ciphertext, :binary
    field :nonce, :binary
    timestamps()
  end
end

defmodule Lumia.Identity.ReauthToken do
  @moduledoc "A five-minute single-purpose token for revoke and link."

  use Lumia.Schema

  @primary_key false
  schema "reauth_tokens" do
    field :token_hash, :binary, primary_key: true
    field :session_id, Ecto.UUID
    field :expires_at, :utc_datetime_usec
    field :created_at, :utc_datetime_usec
  end
end
