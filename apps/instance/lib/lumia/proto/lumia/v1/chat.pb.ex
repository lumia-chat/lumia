defmodule Lumia.V1.ChannelKind do
  @moduledoc false

  use Protobuf,
    enum: true,
    full_name: "lumia.v1.ChannelKind",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :CHANNEL_KIND_UNSPECIFIED, 0
  field :TEXT, 1
  field :SERVER_DM, 2
  field :SERVER_GROUP_DM, 3
  field :PERSONAL_DM, 4
  field :PERSONAL_GROUP_DM, 5
end

defmodule Lumia.V1.AccountKind do
  @moduledoc false

  use Protobuf,
    enum: true,
    full_name: "lumia.v1.AccountKind",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :ACCOUNT_KIND_UNSPECIFIED, 0
  field :ACCOUNT_HUMAN, 1
  field :ACCOUNT_BOT, 2
end

defmodule Lumia.V1.MembershipKind do
  @moduledoc false

  use Protobuf,
    enum: true,
    full_name: "lumia.v1.MembershipKind",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :MEMBERSHIP_KIND_UNSPECIFIED, 0
  field :MEMBER, 1
  field :GUEST, 2
end

defmodule Lumia.V1.PresenceStatus do
  @moduledoc false

  use Protobuf,
    enum: true,
    full_name: "lumia.v1.PresenceStatus",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :PRESENCE_STATUS_UNSPECIFIED, 0
  field :OFFLINE, 1
  field :ONLINE, 2
  field :AWAY, 3
  field :DO_NOT_DISTURB, 4
end

defmodule Lumia.V1.Permission do
  @moduledoc false

  use Protobuf,
    enum: true,
    full_name: "lumia.v1.Permission",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :PERMISSION_UNSPECIFIED, 0
  field :ADMINISTRATOR, 1
  field :MANAGE_SERVER, 2
  field :MANAGE_ROLES, 3
  field :MANAGE_CHANNELS, 4
  field :MANAGE_MEMBERS, 5
  field :CREATE_INVITES, 6
  field :VIEW_CHANNEL, 10
  field :READ_HISTORY, 11
  field :SEND_MESSAGES, 12
  field :SEND_IN_THREADS, 13
  field :ATTACH_FILES, 14
  field :ADD_REACTIONS, 15
  field :MENTION_EVERYONE, 16
  field :MANAGE_MESSAGES, 17
  field :START_CALLS, 20
  field :START_SERVER_DMS, 21
  field :MANAGE_WEBHOOKS, 30
  field :MANAGE_BOTS, 31
end

defmodule Lumia.V1.ChannelNotice.Kind do
  @moduledoc false

  use Protobuf,
    enum: true,
    full_name: "lumia.v1.ChannelNotice.Kind",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :KIND_UNSPECIFIED, 0
  field :JOINED, 1
  field :ADDED, 2
  field :LEFT, 3
  field :REMOVED, 4
end

defmodule Lumia.V1.Account do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.Account",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :account_id, 1, type: :bytes, json_name: "accountId"
  field :identity_id, 2, type: :bytes, json_name: "identityId"
  field :kind, 3, type: Lumia.V1.AccountKind, enum: true
  field :handle, 4, type: :string
  field :display_name, 5, type: :string, json_name: "displayName"
  field :avatar_url, 6, type: :string, json_name: "avatarUrl"
  field :created_at, 7, type: Google.Protobuf.Timestamp, json_name: "createdAt"
  field :profile, 8, type: Lumia.V1.SignedProfile
  field :pronouns, 9, type: :string
  field :bio, 10, type: :string
end

defmodule Lumia.V1.Server do
  @moduledoc false

  use Protobuf, full_name: "lumia.v1.Server", protoc_gen_elixir_version: "0.17.1", syntax: :proto3

  field :server_id, 1, type: :bytes, json_name: "serverId"
  field :name, 2, type: :string
  field :icon_url, 3, type: :string, json_name: "iconUrl"
  field :owner_account_id, 4, type: :bytes, json_name: "ownerAccountId"
  field :created_at, 5, type: Google.Protobuf.Timestamp, json_name: "createdAt"
  field :operator_name, 6, type: :string, json_name: "operatorName"
  field :host, 7, type: :string
  field :retention_days, 8, type: :uint32, json_name: "retentionDays"
end

defmodule Lumia.V1.Role do
  @moduledoc false

  use Protobuf, full_name: "lumia.v1.Role", protoc_gen_elixir_version: "0.17.1", syntax: :proto3

  field :role_id, 1, type: :bytes, json_name: "roleId"
  field :server_id, 2, type: :bytes, json_name: "serverId"
  field :name, 3, type: :string
  field :position, 4, type: :uint32
  field :permissions, 5, repeated: true, type: Lumia.V1.Permission, enum: true
  field :color, 6, type: :string
  field :is_everyone, 7, type: :bool, json_name: "isEveryone"
end

defmodule Lumia.V1.Overwrite do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.Overwrite",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  oneof(:target, 0)

  field :role_id, 1, type: :bytes, json_name: "roleId", oneof: 0
  field :account_id, 2, type: :bytes, json_name: "accountId", oneof: 0
  field :allow, 3, repeated: true, type: Lumia.V1.Permission, enum: true
  field :deny, 4, repeated: true, type: Lumia.V1.Permission, enum: true
end

defmodule Lumia.V1.Channel do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.Channel",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :channel_id, 1, type: :bytes, json_name: "channelId"
  field :kind, 2, type: Lumia.V1.ChannelKind, enum: true
  field :server_id, 3, type: :bytes, json_name: "serverId"
  field :name, 4, type: :string
  field :topic, 5, type: :string
  field :position, 6, type: :uint32
  field :overwrites, 7, repeated: true, type: Lumia.V1.Overwrite

  field :participant_account_ids, 8,
    repeated: true,
    type: :bytes,
    json_name: "participantAccountIds"

  field :last_seq, 9, type: :uint64, json_name: "lastSeq"
  field :acl_version, 10, type: :uint64, json_name: "aclVersion"
  field :created_at, 11, type: Google.Protobuf.Timestamp, json_name: "createdAt"
  field :host, 12, type: :string
  field :private, 13, type: :bool
  field :is_default, 14, type: :bool, json_name: "isDefault"
  field :member_count, 15, type: :uint32, json_name: "memberCount"
end

defmodule Lumia.V1.Member do
  @moduledoc false

  use Protobuf, full_name: "lumia.v1.Member", protoc_gen_elixir_version: "0.17.1", syntax: :proto3

  field :server_id, 1, type: :bytes, json_name: "serverId"
  field :account_id, 2, type: :bytes, json_name: "accountId"
  field :kind, 3, type: Lumia.V1.MembershipKind, enum: true
  field :role_ids, 4, repeated: true, type: :bytes, json_name: "roleIds"
  field :nickname, 5, type: :string
  field :joined_at, 6, type: Google.Protobuf.Timestamp, json_name: "joinedAt"
  field :expires_at, 7, type: Google.Protobuf.Timestamp, json_name: "expiresAt"
  field :permissions, 8, repeated: true, type: Lumia.V1.Permission, enum: true
  field :channel_adds_by_invitation, 9, type: :bool, json_name: "channelAddsByInvitation"
  field :server_dms_off, 10, type: :bool, json_name: "serverDmsOff"
end

defmodule Lumia.V1.Mention do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.Mention",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  oneof(:target, 0)

  field :account_id, 1, type: :bytes, json_name: "accountId", oneof: 0
  field :role_id, 2, type: :bytes, json_name: "roleId", oneof: 0
  field :everyone, 3, type: :bool, oneof: 0
  field :start, 4, type: :uint32
  field :end, 5, type: :uint32
end

defmodule Lumia.V1.Attachment do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.Attachment",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :attachment_id, 1, type: :bytes, json_name: "attachmentId"
  field :content_address, 2, type: :bytes, json_name: "contentAddress"
  field :filename, 3, type: :string
  field :media_type, 4, type: :string, json_name: "mediaType"
  field :size_bytes, 5, type: :uint64, json_name: "sizeBytes"
  field :width, 6, type: :uint32
  field :height, 7, type: :uint32
  field :url, 8, type: :string
  field :thumbnail_url, 9, type: :string, json_name: "thumbnailUrl"
end

defmodule Lumia.V1.ReactionCount do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ReactionCount",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :reaction, 1, type: :string
  field :count, 2, type: :uint32
  field :me, 3, type: :bool
  field :account_ids, 4, repeated: true, type: :bytes, json_name: "accountIds"
end

defmodule Lumia.V1.ThreadSummary do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ThreadSummary",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :reply_count, 1, type: :uint32, json_name: "replyCount"
  field :last_reply_at, 2, type: Google.Protobuf.Timestamp, json_name: "lastReplyAt"

  field :participant_account_ids, 3,
    repeated: true,
    type: :bytes,
    json_name: "participantAccountIds"
end

defmodule Lumia.V1.Message do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.Message",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :message_id, 1, type: :bytes, json_name: "messageId"
  field :channel_id, 2, type: :bytes, json_name: "channelId"
  field :seq, 3, type: :uint64
  field :author_account_id, 4, type: :bytes, json_name: "authorAccountId"
  field :parent_id, 5, type: :bytes, json_name: "parentId"
  field :body, 6, type: :string
  field :mentions, 7, repeated: true, type: Lumia.V1.Mention
  field :attachments, 8, repeated: true, type: Lumia.V1.Attachment
  field :reactions, 9, repeated: true, type: Lumia.V1.ReactionCount
  field :thread, 10, type: Lumia.V1.ThreadSummary
  field :pinned, 11, type: :bool
  field :created_at, 12, type: Google.Protobuf.Timestamp, json_name: "createdAt"
  field :edited_at, 13, type: Google.Protobuf.Timestamp, json_name: "editedAt"
  field :deleted_at, 14, type: Google.Protobuf.Timestamp, json_name: "deletedAt"
  field :mirrors_message_id, 15, type: :bytes, json_name: "mirrorsMessageId"
  field :notice, 16, type: Lumia.V1.ChannelNotice
  field :reply_to, 17, type: Lumia.V1.QuoteReply, json_name: "replyTo"
end

defmodule Lumia.V1.QuoteReply do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.QuoteReply",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :message_id, 1, type: :bytes, json_name: "messageId"
  field :author_account_id, 2, type: :bytes, json_name: "authorAccountId"
  field :text, 3, type: :string
  field :original_deleted, 4, type: :bool, json_name: "originalDeleted"
end

defmodule Lumia.V1.ChannelNotice do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ChannelNotice",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :kind, 1, type: Lumia.V1.ChannelNotice.Kind, enum: true
  field :account_id, 2, type: :bytes, json_name: "accountId"
end

defmodule Lumia.V1.Presence do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.Presence",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :account_id, 1, type: :bytes, json_name: "accountId"
  field :status, 2, type: Lumia.V1.PresenceStatus, enum: true
  field :custom_status, 3, type: :string, json_name: "customStatus"
end

defmodule Lumia.V1.TypingIndicator do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.TypingIndicator",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :channel_id, 1, type: :bytes, json_name: "channelId"
  field :account_id, 2, type: :bytes, json_name: "accountId"
  field :until, 3, type: Google.Protobuf.Timestamp
end
