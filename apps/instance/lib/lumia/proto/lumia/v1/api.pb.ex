defmodule Lumia.V1.PushKind do
  @moduledoc false

  use Protobuf,
    enum: true,
    full_name: "lumia.v1.PushKind",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :PUSH_KIND_UNSPECIFIED, 0
  field :WEB_PUSH, 1
  field :UNIFIED_PUSH, 2
  field :RELAY_APNS, 3
  field :RELAY_FCM, 4
end

defmodule Lumia.V1.TokenPair do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.TokenPair",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :access_token, 1, type: :string, json_name: "accessToken"
  field :access_expires_at, 2, type: Google.Protobuf.Timestamp, json_name: "accessExpiresAt"
  field :refresh_token, 3, type: :string, json_name: "refreshToken"
  field :refresh_expires_at, 4, type: Google.Protobuf.Timestamp, json_name: "refreshExpiresAt"
  field :session_id, 5, type: :bytes, json_name: "sessionId"
end

defmodule Lumia.V1.PasswordSignInRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.PasswordSignInRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :handle_or_email, 1, type: :string, json_name: "handleOrEmail"
  field :password, 2, type: :string
  field :device_id, 3, type: :bytes, json_name: "deviceId"
  field :roster, 4, type: Lumia.V1.SignedRoster
  field :totp_code, 5, type: :string, json_name: "totpCode"
  field :passkey_challenge_id, 6, type: :string, json_name: "passkeyChallengeId"
  field :passkey_credential_json, 7, type: :string, json_name: "passkeyCredentialJson"
end

defmodule Lumia.V1.IdentityProofSignInRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.IdentityProofSignInRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :proof, 1, type: Lumia.V1.IdentityProof
end

defmodule Lumia.V1.TotpEnrollResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.TotpEnrollResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :secret, 1, type: :string
  field :otpauth_uri, 2, type: :string, json_name: "otpauthUri"
end

defmodule Lumia.V1.TotpConfirmRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.TotpConfirmRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :code, 1, type: :string
end

defmodule Lumia.V1.CreateAccountRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.CreateAccountRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :handle, 1, type: :string
  field :email, 2, type: :string
  field :password, 3, type: :string
  field :proof, 4, type: Lumia.V1.IdentityProof
  field :profile, 5, type: Lumia.V1.SignedProfile
  field :invite_code, 6, type: :string, json_name: "inviteCode"
  field :device_id, 7, type: :bytes, json_name: "deviceId"
end

defmodule Lumia.V1.ChallengeRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ChallengeRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :purpose, 1, type: Lumia.V1.ChallengePurpose, enum: true
end

defmodule Lumia.V1.ChallengeResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ChallengeResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :challenge, 1, type: :bytes
  field :expires_at, 2, type: Google.Protobuf.Timestamp, json_name: "expiresAt"
end

defmodule Lumia.V1.RefreshRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.RefreshRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :refresh_token, 1, type: :string, json_name: "refreshToken"
end

defmodule Lumia.V1.Session do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.Session",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :session_id, 1, type: :bytes, json_name: "sessionId"
  field :device_id, 2, type: :bytes, json_name: "deviceId"
  field :client, 3, type: :string
  field :ip, 4, type: :string
  field :created_at, 5, type: Google.Protobuf.Timestamp, json_name: "createdAt"
  field :last_seen_at, 6, type: Google.Protobuf.Timestamp, json_name: "lastSeenAt"
  field :current, 7, type: :bool
end

defmodule Lumia.V1.ListSessionsResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ListSessionsResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :sessions, 1, repeated: true, type: Lumia.V1.Session
end

defmodule Lumia.V1.InstanceInfo do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.InstanceInfo",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :host, 1, type: :string
  field :operator_name, 2, type: :string, json_name: "operatorName"
  field :operator_url, 3, type: :string, json_name: "operatorUrl"
  field :terms_url, 4, type: :string, json_name: "termsUrl"
  field :privacy_url, 5, type: :string, json_name: "privacyUrl"
  field :software_version, 6, type: :string, json_name: "softwareVersion"
  field :identity_proof_sign_in_enabled, 7, type: :bool, json_name: "identityProofSignInEnabled"
  field :identity_proof_create_enabled, 8, type: :bool, json_name: "identityProofCreateEnabled"
  field :open_registration, 9, type: :bool, json_name: "openRegistration"
  field :turn_configured, 10, type: :bool, json_name: "turnConfigured"
  field :push_relay_enabled, 11, type: :bool, json_name: "pushRelayEnabled"
  field :reactions, 12, repeated: true, type: Lumia.V1.ReactionDefinition
  field :recovery_generation, 13, type: :uint64, json_name: "recoveryGeneration"
  field :web_push_public_key, 14, type: :string, json_name: "webPushPublicKey"
  field :magic_link_enabled, 15, type: :bool, json_name: "magicLinkEnabled"
  field :oidc_name, 16, type: :string, json_name: "oidcName"
end

defmodule Lumia.V1.MagicLinkRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.MagicLinkRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :email, 1, type: :string
end

defmodule Lumia.V1.MagicLinkCompleteRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.MagicLinkCompleteRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :token, 1, type: :string
  field :device_id, 2, type: :bytes, json_name: "deviceId"
end

defmodule Lumia.V1.HandoffResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.HandoffResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :token, 1, type: :string
  field :expires_at, 2, type: Google.Protobuf.Timestamp, json_name: "expiresAt"
end

defmodule Lumia.V1.PasskeyChallenge do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.PasskeyChallenge",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :challenge_id, 1, type: :string, json_name: "challengeId"
  field :options_json, 2, type: :string, json_name: "optionsJson"
end

defmodule Lumia.V1.PasskeyRegisterFinishRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.PasskeyRegisterFinishRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :challenge_id, 1, type: :string, json_name: "challengeId"
  field :credential_json, 2, type: :string, json_name: "credentialJson"
  field :label, 3, type: :string
end

defmodule Lumia.V1.PasskeyAssertStartRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.PasskeyAssertStartRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :handle_or_email, 1, type: :string, json_name: "handleOrEmail"
end

defmodule Lumia.V1.PasskeyAssertFinishRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.PasskeyAssertFinishRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :challenge_id, 1, type: :string, json_name: "challengeId"
  field :credential_json, 2, type: :string, json_name: "credentialJson"
  field :device_id, 3, type: :bytes, json_name: "deviceId"
end

defmodule Lumia.V1.Passkey do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.Passkey",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :passkey_id, 1, type: :string, json_name: "passkeyId"
  field :label, 2, type: :string
  field :created_at, 3, type: Google.Protobuf.Timestamp, json_name: "createdAt"
  field :last_used_at, 4, type: Google.Protobuf.Timestamp, json_name: "lastUsedAt"
end

defmodule Lumia.V1.ListPasskeysResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ListPasskeysResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :passkeys, 1, repeated: true, type: Lumia.V1.Passkey
  field :required_for_password, 2, type: :bool, json_name: "requiredForPassword"
end

defmodule Lumia.V1.PasskeyRequirementRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.PasskeyRequirementRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :required, 1, type: :bool
end

defmodule Lumia.V1.PushSubscription do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.PushSubscription",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :kind, 1, type: Lumia.V1.PushKind, enum: true
  field :endpoint, 2, type: :string
  field :p256dh, 3, type: :string
  field :auth, 4, type: :string
end

defmodule Lumia.V1.ReactionDefinition do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ReactionDefinition",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :key, 1, type: :string
  field :label, 2, type: :string
  field :icon_url, 3, type: :string, json_name: "iconUrl"
end

defmodule Lumia.V1.ListServersResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ListServersResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :servers, 1, repeated: true, type: Lumia.V1.Server
  field :memberships, 2, repeated: true, type: Lumia.V1.Member
end

defmodule Lumia.V1.CreateServerRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.CreateServerRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :name, 1, type: :string
end

defmodule Lumia.V1.ListChannelsResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ListChannelsResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :channels, 1, repeated: true, type: Lumia.V1.Channel
end

defmodule Lumia.V1.CreateChannelRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.CreateChannelRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :server_id, 1, type: :bytes, json_name: "serverId"
  field :kind, 2, type: Lumia.V1.ChannelKind, enum: true
  field :name, 3, type: :string
  field :topic, 4, type: :string
  field :overwrites, 5, repeated: true, type: Lumia.V1.Overwrite

  field :participant_account_ids, 6,
    repeated: true,
    type: :bytes,
    json_name: "participantAccountIds"
end

defmodule Lumia.V1.OpenPersonalDmRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.OpenPersonalDmRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :participant_account_ids, 1,
    repeated: true,
    type: :bytes,
    json_name: "participantAccountIds"
end

defmodule Lumia.V1.OpenPersonalDmResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.OpenPersonalDmResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :channel, 1, type: Lumia.V1.Channel
  field :existed, 2, type: :bool
end

defmodule Lumia.V1.ListMembersRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ListMembersRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :server_id, 1, type: :bytes, json_name: "serverId"
  field :page, 2, type: Lumia.V1.PageRequest
  field :query, 3, type: :string
end

defmodule Lumia.V1.ListMembersResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ListMembersResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :members, 1, repeated: true, type: Lumia.V1.Member
  field :accounts, 2, repeated: true, type: Lumia.V1.Account
  field :page, 3, type: Lumia.V1.PageResponse
end

defmodule Lumia.V1.UpdateMemberRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.UpdateMemberRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :server_id, 1, type: :bytes, json_name: "serverId"
  field :account_id, 2, type: :bytes, json_name: "accountId"
  field :role_ids, 3, repeated: true, type: :bytes, json_name: "roleIds"
  field :nickname, 4, type: :string
  field :kind, 5, type: Lumia.V1.MembershipKind, enum: true
  field :expires_at, 6, type: Google.Protobuf.Timestamp, json_name: "expiresAt"
end

defmodule Lumia.V1.ListRolesResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ListRolesResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :roles, 1, repeated: true, type: Lumia.V1.Role
end

defmodule Lumia.V1.UpsertRoleRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.UpsertRoleRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :role, 1, type: Lumia.V1.Role
end

defmodule Lumia.V1.ListEventsRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ListEventsRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :channel_id, 1, type: :bytes, json_name: "channelId"
  field :after_seq, 2, type: :uint64, json_name: "afterSeq"
  field :before_seq, 3, type: :uint64, json_name: "beforeSeq"
  field :limit, 4, type: :uint32
end

defmodule Lumia.V1.ListEventsResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ListEventsResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :events, 1, repeated: true, type: Lumia.V1.Event
  field :last_seq, 2, type: :uint64, json_name: "lastSeq"
  field :truncated_before, 3, type: :bool, json_name: "truncatedBefore"
end

defmodule Lumia.V1.ChannelSnapshot do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ChannelSnapshot",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :channel, 1, type: Lumia.V1.Channel
  field :seq, 2, type: :uint64
  field :messages, 3, repeated: true, type: Lumia.V1.Message
  field :pinned, 4, repeated: true, type: Lumia.V1.Message
  field :encrypted_events, 5, repeated: true, type: Lumia.V1.Event, json_name: "encryptedEvents"
end

defmodule Lumia.V1.ListMessagesRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ListMessagesRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :channel_id, 1, type: :bytes, json_name: "channelId"
  field :before_seq, 2, type: :uint64, json_name: "beforeSeq"
  field :limit, 3, type: :uint32
  field :parent_id, 4, type: :bytes, json_name: "parentId"
end

defmodule Lumia.V1.ListMessagesResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ListMessagesResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :messages, 1, repeated: true, type: Lumia.V1.Message
  field :has_more, 2, type: :bool, json_name: "hasMore"
end

defmodule Lumia.V1.AccountSnapshot do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.AccountSnapshot",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :account, 1, type: Lumia.V1.Account
  field :seq, 2, type: :uint64
  field :servers, 3, repeated: true, type: Lumia.V1.Server
  field :memberships, 4, repeated: true, type: Lumia.V1.Member
  field :channels, 5, repeated: true, type: Lumia.V1.Channel

  field :pending_requests, 6,
    repeated: true,
    type: Lumia.V1.DmRequest,
    json_name: "pendingRequests"

  field :friendships, 7, repeated: true, type: Lumia.V1.FriendshipUpdated
  field :roster, 8, type: Lumia.V1.SignedRoster
  field :blocked_identity_ids, 9, repeated: true, type: :bytes, json_name: "blockedIdentityIds"
end

defmodule Lumia.V1.ListAccountEventsRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ListAccountEventsRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :after_seq, 1, type: :uint64, json_name: "afterSeq"
  field :limit, 2, type: :uint32
end

defmodule Lumia.V1.ListAccountEventsResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ListAccountEventsResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :events, 1, repeated: true, type: Lumia.V1.AccountEvent
  field :last_seq, 2, type: :uint64, json_name: "lastSeq"
  field :truncated_before, 3, type: :bool, json_name: "truncatedBefore"
end

defmodule Lumia.V1.CreateUploadRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.CreateUploadRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :channel_id, 1, type: :bytes, json_name: "channelId"
  field :filename, 2, type: :string
  field :media_type, 3, type: :string, json_name: "mediaType"
  field :size_bytes, 4, type: :uint64, json_name: "sizeBytes"
  field :client_encrypted, 5, type: :bool, json_name: "clientEncrypted"
end

defmodule Lumia.V1.CreateUploadResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.CreateUploadResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :attachment_id, 1, type: :bytes, json_name: "attachmentId"
  field :upload_url, 2, type: :string, json_name: "uploadUrl"
  field :upload_expires_at, 3, type: Google.Protobuf.Timestamp, json_name: "uploadExpiresAt"
end

defmodule Lumia.V1.SearchRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.SearchRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :query, 1, type: :string
  field :server_id, 2, type: :bytes, json_name: "serverId"
  field :channel_id, 3, type: :bytes, json_name: "channelId"
  field :from_account_id, 4, type: :bytes, json_name: "fromAccountId"
  field :page, 5, type: Lumia.V1.PageRequest
end

defmodule Lumia.V1.SearchResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.SearchResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :messages, 1, repeated: true, type: Lumia.V1.Message
  field :page, 2, type: Lumia.V1.PageResponse
end

defmodule Lumia.V1.SendDmRequestRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.SendDmRequestRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :to_account_id, 1, type: :bytes, json_name: "toAccountId"
end

defmodule Lumia.V1.RespondDmRequestRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.RespondDmRequestRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :request_id, 1, type: :bytes, json_name: "requestId"
  field :response, 2, type: Lumia.V1.DmRequestState, enum: true
end

defmodule Lumia.V1.FriendRequestRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.FriendRequestRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :account_id, 1, type: :bytes, json_name: "accountId"
end

defmodule Lumia.V1.ListFriendsResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ListFriendsResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :friends, 1, repeated: true, type: Lumia.V1.Account
  field :states, 2, repeated: true, type: Lumia.V1.FriendshipUpdated
end

defmodule Lumia.V1.UpdateBlockListRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.UpdateBlockListRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :blocked_identity_ids, 1, repeated: true, type: :bytes, json_name: "blockedIdentityIds"
end

defmodule Lumia.V1.PublishRosterRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.PublishRosterRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :roster, 1, type: Lumia.V1.SignedRoster
end

defmodule Lumia.V1.PublishBackupKeyHandoffRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.PublishBackupKeyHandoffRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :handoff, 1, type: Lumia.V1.BackupKeyHandoff
end

defmodule Lumia.V1.RevokeDeviceRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.RevokeDeviceRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :roster, 1, type: Lumia.V1.SignedRoster
  field :reauth_token, 2, type: :string, json_name: "reauthToken"
end

defmodule Lumia.V1.GetRosterResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.GetRosterResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :roster, 1, type: Lumia.V1.SignedRoster
end

defmodule Lumia.V1.ReauthRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ReauthRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  oneof(:credential, 0)

  field :password, 1, type: :string, oneof: 0
  field :totp_code, 2, type: :string, json_name: "totpCode", oneof: 0
  field :passkey_assertion, 3, type: :string, json_name: "passkeyAssertion", oneof: 0
end

defmodule Lumia.V1.ReauthResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ReauthResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :reauth_token, 1, type: :string, json_name: "reauthToken"
  field :expires_at, 2, type: Google.Protobuf.Timestamp, json_name: "expiresAt"
end

defmodule Lumia.V1.CreateLinkBundleRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.CreateLinkBundleRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :ciphertext, 1, type: :bytes
  field :sender_ephemeral_public_key, 2, type: :bytes, json_name: "senderEphemeralPublicKey"
  field :nonce, 3, type: :bytes
  field :device_id, 4, type: :bytes, json_name: "deviceId"
end

defmodule Lumia.V1.CreateLinkBundleResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.CreateLinkBundleResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :code, 1, type: :string
  field :expires_at, 2, type: Google.Protobuf.Timestamp, json_name: "expiresAt"
end

defmodule Lumia.V1.GetLinkBundleResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.GetLinkBundleResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :ciphertext, 1, type: :bytes
  field :sender_ephemeral_public_key, 2, type: :bytes, json_name: "senderEphemeralPublicKey"
  field :nonce, 3, type: :bytes
  field :host, 4, type: :string
end

defmodule Lumia.V1.PutIdentityBackupRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.PutIdentityBackupRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :version, 1, type: :uint64
  field :ciphertext, 2, type: :bytes
  field :nonce, 3, type: :bytes
end

defmodule Lumia.V1.GetIdentityBackupResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.GetIdentityBackupResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :version, 1, type: :uint64
  field :ciphertext, 2, type: :bytes
  field :nonce, 3, type: :bytes
end

defmodule Lumia.V1.PublishKeyPackagesRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.PublishKeyPackagesRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :key_packages, 1, repeated: true, type: :bytes, json_name: "keyPackages"
  field :last_resort_key_package, 2, type: :bytes, json_name: "lastResortKeyPackage"
end

defmodule Lumia.V1.ClaimKeyPackageRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ClaimKeyPackageRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :account_id, 1, type: :bytes, json_name: "accountId"
  field :device_id, 2, type: :bytes, json_name: "deviceId"
end

defmodule Lumia.V1.ClaimKeyPackageResponse.Claimed do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ClaimKeyPackageResponse.Claimed",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :device_id, 1, type: :bytes, json_name: "deviceId"
  field :key_package, 2, type: :bytes, json_name: "keyPackage"
end

defmodule Lumia.V1.ClaimKeyPackageResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ClaimKeyPackageResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :key_packages, 1,
    repeated: true,
    type: Lumia.V1.ClaimKeyPackageResponse.Claimed,
    json_name: "keyPackages"
end

defmodule Lumia.V1.PublishGroupInfoRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.PublishGroupInfoRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :channel_id, 1, type: :bytes, json_name: "channelId"
  field :group_info, 2, type: :bytes, json_name: "groupInfo"
  field :epoch, 3, type: :uint64
end

defmodule Lumia.V1.GetGroupInfoResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.GetGroupInfoResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :group_info, 1, type: :bytes, json_name: "groupInfo"
  field :epoch, 2, type: :uint64
end

defmodule Lumia.V1.Invite do
  @moduledoc false

  use Protobuf, full_name: "lumia.v1.Invite", protoc_gen_elixir_version: "0.17.1", syntax: :proto3

  field :code, 1, type: :string
  field :server_id, 2, type: :bytes, json_name: "serverId"
  field :channel_id, 3, type: :bytes, json_name: "channelId"
  field :created_by, 4, type: :bytes, json_name: "createdBy"
  field :expires_at, 5, type: Google.Protobuf.Timestamp, json_name: "expiresAt"
  field :max_uses, 6, type: :uint32, json_name: "maxUses"
  field :uses, 7, type: :uint32
  field :as_guest, 8, type: :bool, json_name: "asGuest"
  field :guest_expires_at, 9, type: Google.Protobuf.Timestamp, json_name: "guestExpiresAt"
  field :email, 10, type: :string
end

defmodule Lumia.V1.EmailInviteRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.EmailInviteRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :server_id, 1, type: :bytes, json_name: "serverId"
  field :email, 2, type: :string
  field :as_guest, 3, type: :bool, json_name: "asGuest"
  field :guest_days, 4, type: :uint32, json_name: "guestDays"
end

defmodule Lumia.V1.CreateInviteRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.CreateInviteRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :server_id, 1, type: :bytes, json_name: "serverId"
  field :channel_id, 2, type: :bytes, json_name: "channelId"
  field :expires_at, 3, type: Google.Protobuf.Timestamp, json_name: "expiresAt"
  field :max_uses, 4, type: :uint32, json_name: "maxUses"
  field :as_guest, 5, type: :bool, json_name: "asGuest"
  field :guest_days, 6, type: :uint32, json_name: "guestDays"
end

defmodule Lumia.V1.InvitePreview do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.InvitePreview",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :server, 1, type: Lumia.V1.Server
  field :channel, 2, type: Lumia.V1.Channel
  field :member_count, 3, type: :uint32, json_name: "memberCount"
  field :as_guest, 4, type: :bool, json_name: "asGuest"
  field :instance, 5, type: Lumia.V1.InstanceInfo
end

defmodule Lumia.V1.ListInvitesResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ListInvitesResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :invites, 1, repeated: true, type: Lumia.V1.Invite
end
