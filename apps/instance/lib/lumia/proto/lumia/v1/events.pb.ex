defmodule Lumia.V1.LeaveReason do
  @moduledoc false

  use Protobuf,
    enum: true,
    full_name: "lumia.v1.LeaveReason",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :LEAVE_REASON_UNSPECIFIED, 0
  field :LEFT, 1
  field :REMOVED, 2
  field :GUEST_EXPIRED, 3
  field :ACCOUNT_DELETED, 4
end

defmodule Lumia.V1.RedactedKind do
  @moduledoc false

  use Protobuf,
    enum: true,
    full_name: "lumia.v1.RedactedKind",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :REDACTED_KIND_UNSPECIFIED, 0
  field :REDACTED_MESSAGE, 1
  field :REDACTED_OTHER, 2
end

defmodule Lumia.V1.EncryptedKind do
  @moduledoc false

  use Protobuf,
    enum: true,
    full_name: "lumia.v1.EncryptedKind",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :ENCRYPTED_KIND_UNSPECIFIED, 0
  field :APPLICATION, 1
  field :CONTROL, 2
  field :WELCOME, 3
end

defmodule Lumia.V1.DmRequestState do
  @moduledoc false

  use Protobuf,
    enum: true,
    full_name: "lumia.v1.DmRequestState",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :DM_REQUEST_STATE_UNSPECIFIED, 0
  field :PENDING, 1
  field :ACCEPTED, 2
  field :DECLINED, 3
  field :BLOCKED, 4
end

defmodule Lumia.V1.FriendshipState do
  @moduledoc false

  use Protobuf,
    enum: true,
    full_name: "lumia.v1.FriendshipState",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :FRIENDSHIP_STATE_UNSPECIFIED, 0
  field :FRIENDSHIP_REQUESTED_BY_ME, 1
  field :FRIENDSHIP_REQUESTED_BY_THEM, 2
  field :FRIENDSHIP_FRIENDS, 3
  field :FRIENDSHIP_REMOVED, 4
end

defmodule Lumia.V1.Event do
  @moduledoc false

  use Protobuf, full_name: "lumia.v1.Event", protoc_gen_elixir_version: "0.17.1", syntax: :proto3

  oneof(:payload, 0)

  field :event_id, 1, type: :bytes, json_name: "eventId"
  field :channel_id, 2, type: :bytes, json_name: "channelId"
  field :seq, 3, type: :uint64
  field :command_id, 4, type: :bytes, json_name: "commandId"
  field :actor_account_id, 5, type: :bytes, json_name: "actorAccountId"
  field :actor_device_id, 6, type: :bytes, json_name: "actorDeviceId"
  field :created_at, 7, type: Google.Protobuf.Timestamp, json_name: "createdAt"
  field :acl_version, 8, type: :uint64, json_name: "aclVersion"
  field :message_created, 20, type: Lumia.V1.MessageCreated, json_name: "messageCreated", oneof: 0
  field :message_edited, 21, type: Lumia.V1.MessageEdited, json_name: "messageEdited", oneof: 0
  field :message_deleted, 22, type: Lumia.V1.MessageDeleted, json_name: "messageDeleted", oneof: 0
  field :reaction_added, 23, type: Lumia.V1.ReactionAdded, json_name: "reactionAdded", oneof: 0

  field :reaction_removed, 24,
    type: Lumia.V1.ReactionRemoved,
    json_name: "reactionRemoved",
    oneof: 0

  field :message_pinned, 25, type: Lumia.V1.MessagePinned, json_name: "messagePinned", oneof: 0

  field :message_unpinned, 26,
    type: Lumia.V1.MessageUnpinned,
    json_name: "messageUnpinned",
    oneof: 0

  field :thread_updated, 27, type: Lumia.V1.ThreadUpdated, json_name: "threadUpdated", oneof: 0
  field :member_joined, 28, type: Lumia.V1.MemberJoined, json_name: "memberJoined", oneof: 0
  field :member_left, 29, type: Lumia.V1.MemberLeft, json_name: "memberLeft", oneof: 0
  field :channel_updated, 30, type: Lumia.V1.ChannelUpdated, json_name: "channelUpdated", oneof: 0
  field :acl_changed, 31, type: Lumia.V1.AclChanged, json_name: "aclChanged", oneof: 0
  field :redacted, 32, type: Lumia.V1.Redacted, oneof: 0
  field :payloads_purged, 33, type: Lumia.V1.PayloadsPurged, json_name: "payloadsPurged", oneof: 0
  field :encrypted, 40, type: Lumia.V1.EncryptedPayload, oneof: 0
end

defmodule Lumia.V1.PayloadsPurged do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.PayloadsPurged",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :before_seq, 1, type: :uint64, json_name: "beforeSeq"
end

defmodule Lumia.V1.MessageCreated do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.MessageCreated",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :message, 1, type: Lumia.V1.Message
end

defmodule Lumia.V1.MessageEdited do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.MessageEdited",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :message_id, 1, type: :bytes, json_name: "messageId"
  field :body, 2, type: :string
  field :mentions, 3, repeated: true, type: Lumia.V1.Mention
  field :edited_at, 4, type: Google.Protobuf.Timestamp, json_name: "editedAt"
end

defmodule Lumia.V1.MessageDeleted do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.MessageDeleted",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :message_id, 1, type: :bytes, json_name: "messageId"
  field :purge_cache, 2, type: :bool, json_name: "purgeCache"
end

defmodule Lumia.V1.ReactionAdded do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ReactionAdded",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :message_id, 1, type: :bytes, json_name: "messageId"
  field :reaction, 2, type: :string
  field :account_id, 3, type: :bytes, json_name: "accountId"
end

defmodule Lumia.V1.ReactionRemoved do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ReactionRemoved",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :message_id, 1, type: :bytes, json_name: "messageId"
  field :reaction, 2, type: :string
  field :account_id, 3, type: :bytes, json_name: "accountId"
end

defmodule Lumia.V1.MessagePinned do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.MessagePinned",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :message_id, 1, type: :bytes, json_name: "messageId"
end

defmodule Lumia.V1.MessageUnpinned do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.MessageUnpinned",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :message_id, 1, type: :bytes, json_name: "messageId"
end

defmodule Lumia.V1.ThreadUpdated do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ThreadUpdated",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :message_id, 1, type: :bytes, json_name: "messageId"
  field :thread, 2, type: Lumia.V1.ThreadSummary
end

defmodule Lumia.V1.MemberJoined do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.MemberJoined",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :account_id, 1, type: :bytes, json_name: "accountId"
end

defmodule Lumia.V1.MemberLeft do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.MemberLeft",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :account_id, 1, type: :bytes, json_name: "accountId"
  field :reason, 2, type: Lumia.V1.LeaveReason, enum: true
end

defmodule Lumia.V1.ChannelUpdated do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ChannelUpdated",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :channel, 1, type: Lumia.V1.Channel
end

defmodule Lumia.V1.AclChanged do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.AclChanged",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :acl_version, 1, type: :uint64, json_name: "aclVersion"
end

defmodule Lumia.V1.Redacted do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.Redacted",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :was, 1, type: Lumia.V1.RedactedKind, enum: true
end

defmodule Lumia.V1.EncryptedPayload do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.EncryptedPayload",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :kind, 1, type: Lumia.V1.EncryptedKind, enum: true
  field :mls_message, 2, type: :bytes, json_name: "mlsMessage"
  field :epoch, 3, type: :uint64
end

defmodule Lumia.V1.AccountEvent do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.AccountEvent",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  oneof(:payload, 0)

  field :event_id, 1, type: :bytes, json_name: "eventId"
  field :account_id, 2, type: :bytes, json_name: "accountId"
  field :seq, 3, type: :uint64
  field :created_at, 4, type: Google.Protobuf.Timestamp, json_name: "createdAt"

  field :membership_added, 20,
    type: Lumia.V1.MembershipAdded,
    json_name: "membershipAdded",
    oneof: 0

  field :membership_removed, 21,
    type: Lumia.V1.MembershipRemoved,
    json_name: "membershipRemoved",
    oneof: 0

  field :membership_updated, 22,
    type: Lumia.V1.MembershipUpdated,
    json_name: "membershipUpdated",
    oneof: 0

  field :channel_added, 23, type: Lumia.V1.ChannelAdded, json_name: "channelAdded", oneof: 0
  field :channel_removed, 24, type: Lumia.V1.ChannelRemoved, json_name: "channelRemoved", oneof: 0

  field :dm_request_received, 25,
    type: Lumia.V1.DmRequestReceived,
    json_name: "dmRequestReceived",
    oneof: 0

  field :dm_request_updated, 26,
    type: Lumia.V1.DmRequestUpdated,
    json_name: "dmRequestUpdated",
    oneof: 0

  field :friendship_updated, 27,
    type: Lumia.V1.FriendshipUpdated,
    json_name: "friendshipUpdated",
    oneof: 0

  field :roster_updated, 28, type: Lumia.V1.RosterUpdated, json_name: "rosterUpdated", oneof: 0

  field :contact_roster_updated, 29,
    type: Lumia.V1.ContactRosterUpdated,
    json_name: "contactRosterUpdated",
    oneof: 0

  field :profile_updated, 30, type: Lumia.V1.ProfileUpdated, json_name: "profileUpdated", oneof: 0
  field :session_revoked, 31, type: Lumia.V1.SessionRevoked, json_name: "sessionRevoked", oneof: 0

  field :block_list_updated, 32,
    type: Lumia.V1.BlockListUpdated,
    json_name: "blockListUpdated",
    oneof: 0

  field :backup_key_handed_off, 33,
    type: Lumia.V1.BackupKeyHandedOff,
    json_name: "backupKeyHandedOff",
    oneof: 0

  field :server_updated, 34, type: Lumia.V1.ServerUpdated, json_name: "serverUpdated", oneof: 0

  field :channel_invited, 35,
    type: Lumia.V1.ChannelInvitation,
    json_name: "channelInvited",
    oneof: 0

  field :channel_invitation_removed, 36,
    type: Lumia.V1.ChannelInvitationRemoved,
    json_name: "channelInvitationRemoved",
    oneof: 0
end

defmodule Lumia.V1.MembershipAdded do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.MembershipAdded",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :member, 1, type: Lumia.V1.Member
  field :server, 2, type: Lumia.V1.Server
end

defmodule Lumia.V1.MembershipRemoved do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.MembershipRemoved",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :server_id, 1, type: :bytes, json_name: "serverId"
  field :reason, 2, type: Lumia.V1.LeaveReason, enum: true
end

defmodule Lumia.V1.MembershipUpdated do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.MembershipUpdated",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :member, 1, type: Lumia.V1.Member
end

defmodule Lumia.V1.ChannelAdded do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ChannelAdded",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :channel, 1, type: Lumia.V1.Channel
end

defmodule Lumia.V1.ChannelRemoved do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ChannelRemoved",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :channel_id, 1, type: :bytes, json_name: "channelId"
end

defmodule Lumia.V1.ServerUpdated do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ServerUpdated",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :server, 1, type: Lumia.V1.Server
end

defmodule Lumia.V1.ChannelInvitation do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ChannelInvitation",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :channel, 1, type: Lumia.V1.Channel
  field :invited_by_account_id, 2, type: :bytes, json_name: "invitedByAccountId"
  field :created_at, 3, type: Google.Protobuf.Timestamp, json_name: "createdAt"
end

defmodule Lumia.V1.ChannelInvitationRemoved do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ChannelInvitationRemoved",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :channel_id, 1, type: :bytes, json_name: "channelId"
end

defmodule Lumia.V1.DmRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.DmRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :request_id, 1, type: :bytes, json_name: "requestId"
  field :from_account_id, 2, type: :bytes, json_name: "fromAccountId"
  field :to_account_id, 3, type: :bytes, json_name: "toAccountId"
  field :state, 4, type: Lumia.V1.DmRequestState, enum: true
  field :shared_server_ids, 5, repeated: true, type: :bytes, json_name: "sharedServerIds"
  field :mutual_friend_count, 6, type: :uint32, json_name: "mutualFriendCount"
  field :from_profile, 7, type: Lumia.V1.SignedProfile, json_name: "fromProfile"
  field :created_at, 8, type: Google.Protobuf.Timestamp, json_name: "createdAt"
end

defmodule Lumia.V1.DmRequestReceived do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.DmRequestReceived",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :request, 1, type: Lumia.V1.DmRequest
end

defmodule Lumia.V1.DmRequestUpdated do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.DmRequestUpdated",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :request, 1, type: Lumia.V1.DmRequest
end

defmodule Lumia.V1.FriendshipUpdated do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.FriendshipUpdated",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :other_account_id, 1, type: :bytes, json_name: "otherAccountId"
  field :state, 2, type: Lumia.V1.FriendshipState, enum: true
end

defmodule Lumia.V1.RosterUpdated do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.RosterUpdated",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :roster, 1, type: Lumia.V1.SignedRoster
end

defmodule Lumia.V1.ContactRosterUpdated do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ContactRosterUpdated",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :identity_id, 1, type: :bytes, json_name: "identityId"
  field :roster, 2, type: Lumia.V1.SignedRoster
end

defmodule Lumia.V1.ProfileUpdated do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ProfileUpdated",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :account, 1, type: Lumia.V1.Account
end

defmodule Lumia.V1.SessionRevoked do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.SessionRevoked",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :session_id, 1, type: :bytes, json_name: "sessionId"
  field :device_id, 2, type: :bytes, json_name: "deviceId"
end

defmodule Lumia.V1.BlockListUpdated do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.BlockListUpdated",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :blocked_identity_ids, 1, repeated: true, type: :bytes, json_name: "blockedIdentityIds"
end

defmodule Lumia.V1.BackupKeyHandedOff do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.BackupKeyHandedOff",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :handoff, 1, type: Lumia.V1.BackupKeyHandoff
end
