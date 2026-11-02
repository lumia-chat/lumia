defmodule Lumia.V1.SubscribeStatus do
  @moduledoc false

  use Protobuf,
    enum: true,
    full_name: "lumia.v1.SubscribeStatus",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :SUBSCRIBE_STATUS_UNSPECIFIED, 0
  field :RESUMED, 1
  field :SNAPSHOT_REQUIRED, 2
  field :ERROR, 3
end

defmodule Lumia.V1.DisconnectReason do
  @moduledoc false

  use Protobuf,
    enum: true,
    full_name: "lumia.v1.DisconnectReason",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :DISCONNECT_REASON_UNSPECIFIED, 0
  field :DISCONNECT_SERVER_SHUTDOWN, 1
  field :DISCONNECT_SESSION_REVOKED, 2
  field :DISCONNECT_DEVICE_REVOKED, 3
  field :DISCONNECT_HEARTBEAT_TIMEOUT, 4
  field :DISCONNECT_PROTOCOL_ERROR, 5
  field :DISCONNECT_RATE_LIMITED, 6
  field :DISCONNECT_SLOW_CONSUMER, 7
end

defmodule Lumia.V1.ClientFrame do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ClientFrame",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  oneof(:frame, 0)

  field :hello, 1, type: Lumia.V1.Hello, oneof: 0
  field :subscribe, 2, type: Lumia.V1.Subscribe, oneof: 0
  field :unsubscribe, 3, type: Lumia.V1.Unsubscribe, oneof: 0
  field :command, 4, type: Lumia.V1.Command, oneof: 0

  field :high_water_mark, 5,
    type: Lumia.V1.HighWaterMarkRequest,
    json_name: "highWaterMark",
    oneof: 0

  field :heartbeat, 6, type: Lumia.V1.Heartbeat, oneof: 0
  field :presence, 7, type: Lumia.V1.PresenceUpdate, oneof: 0
  field :typing, 8, type: Lumia.V1.Typing, oneof: 0
  field :resume_account, 9, type: Lumia.V1.ResumeAccount, json_name: "resumeAccount", oneof: 0
end

defmodule Lumia.V1.ServerFrame do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ServerFrame",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  oneof(:frame, 0)

  field :welcome, 1, type: Lumia.V1.Welcome, oneof: 0

  field :subscribe_result, 2,
    type: Lumia.V1.SubscribeResult,
    json_name: "subscribeResult",
    oneof: 0

  field :event, 3, type: Lumia.V1.Event, oneof: 0
  field :ack, 4, type: Lumia.V1.Ack, oneof: 0
  field :error, 5, type: Lumia.V1.Error, oneof: 0
  field :nudge, 6, type: Lumia.V1.Nudge, oneof: 0

  field :high_water_mark, 7,
    type: Lumia.V1.HighWaterMarkResponse,
    json_name: "highWaterMark",
    oneof: 0

  field :heartbeat, 8, type: Lumia.V1.Heartbeat, oneof: 0
  field :presence_diff, 9, type: Lumia.V1.PresenceDiff, json_name: "presenceDiff", oneof: 0
  field :typing, 10, type: Lumia.V1.TypingIndicator, oneof: 0
  field :account_event, 11, type: Lumia.V1.AccountEvent, json_name: "accountEvent", oneof: 0

  field :account_resume_result, 12,
    type: Lumia.V1.AccountResumeResult,
    json_name: "accountResumeResult",
    oneof: 0

  field :disconnect, 13, type: Lumia.V1.Disconnect, oneof: 0
end

defmodule Lumia.V1.Hello do
  @moduledoc false

  use Protobuf, full_name: "lumia.v1.Hello", protoc_gen_elixir_version: "0.17.1", syntax: :proto3

  field :client, 1, type: :string
  field :device_id, 2, type: :bytes, json_name: "deviceId"
  field :recovery_generation, 3, type: :uint64, json_name: "recoveryGeneration"
  field :roster_version, 4, type: :uint64, json_name: "rosterVersion"
end

defmodule Lumia.V1.Welcome do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.Welcome",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :session_id, 1, type: :bytes, json_name: "sessionId"
  field :account_id, 2, type: :bytes, json_name: "accountId"
  field :recovery_generation, 3, type: :uint64, json_name: "recoveryGeneration"
  field :server_time, 4, type: Google.Protobuf.Timestamp, json_name: "serverTime"
  field :heartbeat_interval_seconds, 5, type: :uint32, json_name: "heartbeatIntervalSeconds"
  field :turn_configured, 6, type: :bool, json_name: "turnConfigured"
  field :push_relay_enabled, 7, type: :bool, json_name: "pushRelayEnabled"
  field :identity_proof_sign_in_enabled, 8, type: :bool, json_name: "identityProofSignInEnabled"
  field :limits, 9, type: Lumia.V1.Limits
  field :roster_update_available, 10, type: :bool, json_name: "rosterUpdateAvailable"
  field :key_package_count, 11, type: :uint32, json_name: "keyPackageCount"
end

defmodule Lumia.V1.Limits do
  @moduledoc false

  use Protobuf, full_name: "lumia.v1.Limits", protoc_gen_elixir_version: "0.17.1", syntax: :proto3

  field :max_message_bytes, 1, type: :uint32, json_name: "maxMessageBytes"
  field :max_upload_bytes, 2, type: :uint64, json_name: "maxUploadBytes"
  field :max_subscriptions, 3, type: :uint32, json_name: "maxSubscriptions"
  field :max_commands_per_minute, 4, type: :uint32, json_name: "maxCommandsPerMinute"
end

defmodule Lumia.V1.Subscribe do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.Subscribe",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :cursors, 1, repeated: true, type: Lumia.V1.Cursor
end

defmodule Lumia.V1.Unsubscribe do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.Unsubscribe",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :channel_ids, 1, repeated: true, type: :bytes, json_name: "channelIds"
end

defmodule Lumia.V1.SubscribeResult do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.SubscribeResult",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :results, 1, repeated: true, type: Lumia.V1.ChannelSubscribeResult
end

defmodule Lumia.V1.ChannelSubscribeResult do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ChannelSubscribeResult",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :channel_id, 1, type: :bytes, json_name: "channelId"
  field :status, 2, type: Lumia.V1.SubscribeStatus, enum: true
  field :last_seq, 3, type: :uint64, json_name: "lastSeq"
  field :acl_version, 4, type: :uint64, json_name: "aclVersion"
  field :error, 5, type: Lumia.V1.Error
end

defmodule Lumia.V1.ResumeAccount do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ResumeAccount",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :after_seq, 1, type: :uint64, json_name: "afterSeq"
end

defmodule Lumia.V1.AccountResumeResult do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.AccountResumeResult",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :status, 1, type: Lumia.V1.SubscribeStatus, enum: true
  field :last_seq, 2, type: :uint64, json_name: "lastSeq"
  field :error, 3, type: Lumia.V1.Error
end

defmodule Lumia.V1.Nudge do
  @moduledoc false

  use Protobuf, full_name: "lumia.v1.Nudge", protoc_gen_elixir_version: "0.17.1", syntax: :proto3

  field :heads, 1, repeated: true, type: Lumia.V1.Cursor
end

defmodule Lumia.V1.HighWaterMarkRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.HighWaterMarkRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :channel_ids, 1, repeated: true, type: :bytes, json_name: "channelIds"
  field :include_account_log, 2, type: :bool, json_name: "includeAccountLog"
end

defmodule Lumia.V1.HighWaterMarkResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.HighWaterMarkResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :heads, 1, repeated: true, type: Lumia.V1.Cursor
  field :account_log_seq, 2, type: :uint64, json_name: "accountLogSeq"
end

defmodule Lumia.V1.Heartbeat do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.Heartbeat",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :nonce, 1, type: :uint64
end

defmodule Lumia.V1.PresenceUpdate do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.PresenceUpdate",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :status, 1, type: Lumia.V1.PresenceStatus, enum: true
  field :custom_status, 2, type: :string, json_name: "customStatus"
  field :watch_server_ids, 3, repeated: true, type: :bytes, json_name: "watchServerIds"
end

defmodule Lumia.V1.PresenceDiff do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.PresenceDiff",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :server_id, 1, type: :bytes, json_name: "serverId"
  field :joined, 2, repeated: true, type: Lumia.V1.Presence
  field :updated, 3, repeated: true, type: Lumia.V1.Presence
  field :left_account_ids, 4, repeated: true, type: :bytes, json_name: "leftAccountIds"
end

defmodule Lumia.V1.Typing do
  @moduledoc false

  use Protobuf, full_name: "lumia.v1.Typing", protoc_gen_elixir_version: "0.17.1", syntax: :proto3

  field :channel_id, 1, type: :bytes, json_name: "channelId"
end

defmodule Lumia.V1.Disconnect do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.Disconnect",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :reason, 1, type: Lumia.V1.DisconnectReason, enum: true
  field :reconnect_after_seconds, 2, type: :uint32, json_name: "reconnectAfterSeconds"
  field :error, 3, type: Lumia.V1.Error
end
