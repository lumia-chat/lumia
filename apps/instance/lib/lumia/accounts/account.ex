defmodule Lumia.Accounts.Account do
  @moduledoc """
  An account on this instance, bound to an identity (architecture 4, 6.2).

  `kind` is `human` or `bot`. Guest status is a property of membership, not
  of the account. `instance_role` is the instance-administration layer
  (docs/specs/authorization.md section 7). `last_seq` and `last_seq_at` are
  the head of this account's log.
  """

  use Lumia.Schema

  @type t :: %__MODULE__{}

  schema "accounts" do
    field :identity_id, :binary
    field :kind, :string, default: "human"
    field :handle, :string
    field :display_name, :string
    field :avatar_url, :string
    field :email, :string
    field :password_hash, :string, redact: true
    field :instance_role, :string, default: "none"
    field :last_seq, :integer, default: 0
    # Second granularity; see the migration for why.
    field :last_seq_at, :utc_datetime
    field :suspended_at, :utc_datetime_usec
    field :deleted_at, :utc_datetime_usec
    timestamps()
  end
end
