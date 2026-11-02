defmodule Lumia.V1.SignerKind do
  @moduledoc false

  use Protobuf,
    enum: true,
    full_name: "lumia.v1.SignerKind",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :SIGNER_KIND_UNSPECIFIED, 0
  field :ROOT, 1
  field :DEVICE, 2
end

defmodule Lumia.V1.DeviceKind do
  @moduledoc false

  use Protobuf,
    enum: true,
    full_name: "lumia.v1.DeviceKind",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :DEVICE_KIND_UNSPECIFIED, 0
  field :NATIVE, 1
  field :WEB, 2
  field :BOT, 3
end

defmodule Lumia.V1.ChallengePurpose do
  @moduledoc false

  use Protobuf,
    enum: true,
    full_name: "lumia.v1.ChallengePurpose",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :CHALLENGE_PURPOSE_UNSPECIFIED, 0
  field :CREATE_ACCOUNT, 1
  field :SIGN_IN, 2
  field :LINK_ACCOUNT, 3
  field :JOIN_AS_GUEST, 4
end

defmodule Lumia.V1.PublicKey do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.PublicKey",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :ed25519, 1, type: :bytes
end

defmodule Lumia.V1.Signature do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.Signature",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :signer_kind, 1, type: Lumia.V1.SignerKind, json_name: "signerKind", enum: true
  field :signer_id, 2, type: :bytes, json_name: "signerId"
  field :signature, 3, type: :bytes
end

defmodule Lumia.V1.DeviceCertBody do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.DeviceCertBody",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :identity_id, 1, type: :bytes, json_name: "identityId"
  field :device_id, 2, type: :bytes, json_name: "deviceId"
  field :public_key, 3, type: Lumia.V1.PublicKey, json_name: "publicKey"
  field :kind, 4, type: Lumia.V1.DeviceKind, enum: true
  field :added_at, 5, type: Google.Protobuf.Timestamp, json_name: "addedAt"
  field :added_by, 6, type: :bytes, json_name: "addedBy"
  field :web_origin, 7, type: :string, json_name: "webOrigin"
end

defmodule Lumia.V1.DeviceEntry do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.DeviceEntry",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :cert_body, 1, type: :bytes, json_name: "certBody"
  field :authorization, 2, type: Lumia.V1.Signature
  field :device_id, 3, type: :bytes, json_name: "deviceId"
  field :public_key, 4, type: Lumia.V1.PublicKey, json_name: "publicKey"
  field :kind, 5, type: Lumia.V1.DeviceKind, enum: true
  field :added_at, 6, type: Google.Protobuf.Timestamp, json_name: "addedAt"
  field :added_by, 7, type: :bytes, json_name: "addedBy"
  field :web_origin, 8, type: :string, json_name: "webOrigin"
  field :label, 9, type: :string
end

defmodule Lumia.V1.RevocationBody do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.RevocationBody",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :identity_id, 1, type: :bytes, json_name: "identityId"
  field :device_id, 2, type: :bytes, json_name: "deviceId"
  field :revoked_at, 3, type: Google.Protobuf.Timestamp, json_name: "revokedAt"
  field :revoked_by, 4, type: :bytes, json_name: "revokedBy"
end

defmodule Lumia.V1.RevokedEntry do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.RevokedEntry",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :body, 1, type: :bytes
  field :signature, 2, type: Lumia.V1.Signature
  field :device_id, 3, type: :bytes, json_name: "deviceId"
  field :revoked_at, 4, type: Google.Protobuf.Timestamp, json_name: "revokedAt"
  field :revoked_by, 5, type: :bytes, json_name: "revokedBy"
end

defmodule Lumia.V1.RootRotationBody do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.RootRotationBody",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :identity_id, 1, type: :bytes, json_name: "identityId"
  field :from, 2, type: Lumia.V1.PublicKey
  field :to, 3, type: Lumia.V1.PublicKey
  field :rotated_at, 4, type: Google.Protobuf.Timestamp, json_name: "rotatedAt"
end

defmodule Lumia.V1.RootRotation do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.RootRotation",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :body, 1, type: :bytes
  field :signature, 2, type: Lumia.V1.Signature
end

defmodule Lumia.V1.RosterBody do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.RosterBody",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :identity_id, 1, type: :bytes, json_name: "identityId"
  field :version, 2, type: :uint64
  field :root_public_key, 3, type: Lumia.V1.PublicKey, json_name: "rootPublicKey"
  field :rotations, 4, repeated: true, type: Lumia.V1.RootRotation
  field :devices, 5, repeated: true, type: Lumia.V1.DeviceEntry
  field :revoked, 6, repeated: true, type: Lumia.V1.RevokedEntry
  field :issued_at, 7, type: Google.Protobuf.Timestamp, json_name: "issuedAt"
end

defmodule Lumia.V1.SignedRoster do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.SignedRoster",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :body, 1, type: :bytes
  field :signatures, 2, repeated: true, type: Lumia.V1.Signature
end

defmodule Lumia.V1.IdentityChallengeBody do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.IdentityChallengeBody",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :nonce, 1, type: :bytes
  field :host, 2, type: :string
  field :expires_at, 3, type: Google.Protobuf.Timestamp, json_name: "expiresAt"
  field :purpose, 4, type: Lumia.V1.ChallengePurpose, enum: true
  field :mac, 5, type: :bytes
end

defmodule Lumia.V1.IdentityProof do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.IdentityProof",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :challenge, 1, type: :bytes
  field :identity_id, 2, type: :bytes, json_name: "identityId"
  field :device_id, 3, type: :bytes, json_name: "deviceId"
  field :signature, 4, type: Lumia.V1.Signature
  field :roster, 5, type: Lumia.V1.SignedRoster
end

defmodule Lumia.V1.ProfileBody do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ProfileBody",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :identity_id, 1, type: :bytes, json_name: "identityId"
  field :version, 2, type: :uint64
  field :display_name, 3, type: :string, json_name: "displayName"
  field :pronouns, 4, type: :string
  field :bio, 5, type: :string
  field :avatar_url, 6, type: :string, json_name: "avatarUrl"
  field :issued_at, 7, type: Google.Protobuf.Timestamp, json_name: "issuedAt"
end

defmodule Lumia.V1.SignedProfile do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.SignedProfile",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :body, 1, type: :bytes
  field :signature, 2, type: Lumia.V1.Signature
end

defmodule Lumia.V1.LinkRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.LinkRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :device_id, 1, type: :bytes, json_name: "deviceId"
  field :device_public_key, 2, type: Lumia.V1.PublicKey, json_name: "devicePublicKey"
  field :ephemeral_public_key, 3, type: :bytes, json_name: "ephemeralPublicKey"
  field :kind, 4, type: Lumia.V1.DeviceKind, enum: true
  field :label, 5, type: :string
  field :web_origin, 6, type: :string, json_name: "webOrigin"
  field :nonce, 7, type: :bytes
end

defmodule Lumia.V1.LinkBundleBody do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.LinkBundleBody",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :roster, 1, type: Lumia.V1.SignedRoster
  field :backup_key, 2, type: :bytes, json_name: "backupKey"
  field :accounts, 3, repeated: true, type: Lumia.V1.HostAccount
  field :pinned_keys, 4, repeated: true, type: Lumia.V1.PinnedKey, json_name: "pinnedKeys"
  field :nonce, 5, type: :bytes
end

defmodule Lumia.V1.HostAccount do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.HostAccount",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :host, 1, type: :string
  field :account_id, 2, type: :bytes, json_name: "accountId"
end

defmodule Lumia.V1.PinnedKey do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.PinnedKey",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :identity_id, 1, type: :bytes, json_name: "identityId"
  field :root_public_key, 2, type: Lumia.V1.PublicKey, json_name: "rootPublicKey"
  field :roster_version, 3, type: :uint64, json_name: "rosterVersion"
  field :verified, 4, type: :bool
  field :pinned_at, 5, type: Google.Protobuf.Timestamp, json_name: "pinnedAt"
end

defmodule Lumia.V1.IdentityBackupBody do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.IdentityBackupBody",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :version, 1, type: :uint64
  field :roster, 2, type: Lumia.V1.SignedRoster
  field :profile, 3, type: Lumia.V1.SignedProfile
  field :accounts, 4, repeated: true, type: Lumia.V1.HostAccount
  field :pinned_keys, 5, repeated: true, type: Lumia.V1.PinnedKey, json_name: "pinnedKeys"
  field :blocked_identity_ids, 6, repeated: true, type: :bytes, json_name: "blockedIdentityIds"
  field :written_at, 7, type: Google.Protobuf.Timestamp, json_name: "writtenAt"
end
