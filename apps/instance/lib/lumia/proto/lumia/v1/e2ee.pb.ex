defmodule Lumia.V1.DmOperation do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.DmOperation",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  oneof(:op, 0)

  field :operation_id, 1, type: :bytes, json_name: "operationId"
  field :sent_at, 2, type: Google.Protobuf.Timestamp, json_name: "sentAt"
  field :send, 10, type: Lumia.V1.DmSend, oneof: 0
  field :edit, 11, type: Lumia.V1.DmEdit, oneof: 0
  field :delete, 12, type: Lumia.V1.DmDelete, oneof: 0
  field :react, 13, type: Lumia.V1.DmReact, oneof: 0
  field :pin, 14, type: Lumia.V1.DmPin, oneof: 0
  field :read, 15, type: Lumia.V1.DmRead, oneof: 0
  field :typing, 16, type: Lumia.V1.DmTyping, oneof: 0
end

defmodule Lumia.V1.DmSend do
  @moduledoc false

  use Protobuf, full_name: "lumia.v1.DmSend", protoc_gen_elixir_version: "0.17.1", syntax: :proto3

  field :message_id, 1, type: :bytes, json_name: "messageId"
  field :body, 2, type: :string
  field :parent_id, 3, type: :bytes, json_name: "parentId"
  field :attachments, 4, repeated: true, type: Lumia.V1.DmAttachment
  field :mentions, 5, repeated: true, type: :bytes
end

defmodule Lumia.V1.DmEdit do
  @moduledoc false

  use Protobuf, full_name: "lumia.v1.DmEdit", protoc_gen_elixir_version: "0.17.1", syntax: :proto3

  field :message_id, 1, type: :bytes, json_name: "messageId"
  field :body, 2, type: :string
end

defmodule Lumia.V1.DmDelete do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.DmDelete",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :message_id, 1, type: :bytes, json_name: "messageId"
end

defmodule Lumia.V1.DmReact do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.DmReact",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :message_id, 1, type: :bytes, json_name: "messageId"
  field :emoji, 2, type: :string
  field :remove, 3, type: :bool
end

defmodule Lumia.V1.DmPin do
  @moduledoc false

  use Protobuf, full_name: "lumia.v1.DmPin", protoc_gen_elixir_version: "0.17.1", syntax: :proto3

  field :message_id, 1, type: :bytes, json_name: "messageId"
  field :unpin, 2, type: :bool
end

defmodule Lumia.V1.DmRead do
  @moduledoc false

  use Protobuf, full_name: "lumia.v1.DmRead", protoc_gen_elixir_version: "0.17.1", syntax: :proto3

  field :through_seq, 1, type: :uint64, json_name: "throughSeq"
end

defmodule Lumia.V1.DmTyping do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.DmTyping",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :until, 1, type: Google.Protobuf.Timestamp
end

defmodule Lumia.V1.DmAttachment do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.DmAttachment",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :attachment_id, 1, type: :bytes, json_name: "attachmentId"
  field :name, 2, type: :string
  field :content_type, 3, type: :string, json_name: "contentType"
  field :size, 4, type: :uint64
  field :key, 5, type: :bytes
  field :nonce, 6, type: :bytes
  field :hash, 7, type: :bytes
end

defmodule Lumia.V1.HistoryChunkBody do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.HistoryChunkBody",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :channel_id, 1, type: :bytes, json_name: "channelId"
  field :host, 2, type: :string
  field :from_seq, 3, type: :uint64, json_name: "fromSeq"
  field :through_seq, 4, type: :uint64, json_name: "throughSeq"
  field :entries, 5, repeated: true, type: Lumia.V1.HistoryEntry
end

defmodule Lumia.V1.HistoryEntry do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.HistoryEntry",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :seq, 1, type: :uint64
  field :sender_identity_id, 2, type: :bytes, json_name: "senderIdentityId"
  field :sender_device_id, 3, type: :bytes, json_name: "senderDeviceId"
  field :operation, 4, type: Lumia.V1.DmOperation
end

defmodule Lumia.V1.HistoryManifestBody do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.HistoryManifestBody",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :identity_id, 1, type: :bytes, json_name: "identityId"
  field :device_id, 2, type: :bytes, json_name: "deviceId"
  field :version, 3, type: :uint64
  field :written_at, 4, type: Google.Protobuf.Timestamp, json_name: "writtenAt"
  field :chunks, 5, repeated: true, type: Lumia.V1.HistoryManifestEntry
end

defmodule Lumia.V1.HistoryManifestEntry do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.HistoryManifestEntry",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :chunk_id, 1, type: :bytes, json_name: "chunkId"
  field :channel_id, 2, type: :bytes, json_name: "channelId"
  field :host, 3, type: :string
  field :from_seq, 4, type: :uint64, json_name: "fromSeq"
  field :through_seq, 5, type: :uint64, json_name: "throughSeq"
  field :wrapped_key, 6, type: :bytes, json_name: "wrappedKey"
  field :wrap_nonce, 7, type: :bytes, json_name: "wrapNonce"
  field :size, 8, type: :uint64
end

defmodule Lumia.V1.SignedHistoryManifest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.SignedHistoryManifest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :body, 1, type: :bytes
  field :signature, 2, type: Lumia.V1.Signature
end

defmodule Lumia.V1.PutHistoryChunkRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.PutHistoryChunkRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :channel_id, 1, type: :bytes, json_name: "channelId"
  field :ciphertext, 2, type: :bytes
  field :nonce, 3, type: :bytes
end

defmodule Lumia.V1.GetHistoryChunkResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.GetHistoryChunkResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :ciphertext, 1, type: :bytes
  field :nonce, 2, type: :bytes
end

defmodule Lumia.V1.PutHistoryManifestRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.PutHistoryManifestRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :manifest, 1, type: Lumia.V1.SignedHistoryManifest
end

defmodule Lumia.V1.ListHistoryManifestsResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ListHistoryManifestsResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :manifests, 1, repeated: true, type: Lumia.V1.SignedHistoryManifest
end

defmodule Lumia.V1.HistoryUsageResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.HistoryUsageResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :bytes_used, 1, type: :uint64, json_name: "bytesUsed"
  field :bytes_quota, 2, type: :uint64, json_name: "bytesQuota"
end
