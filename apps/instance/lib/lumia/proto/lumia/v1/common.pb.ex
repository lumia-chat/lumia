defmodule Lumia.V1.ErrorCode do
  @moduledoc false

  use Protobuf,
    enum: true,
    full_name: "lumia.v1.ErrorCode",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :ERROR_CODE_UNSPECIFIED, 0
  field :INVALID_ARGUMENT, 1
  field :FAILED_PRECONDITION, 2
  field :CONFLICT, 3
  field :ALREADY_EXISTS, 4
  field :CURSOR_EXPIRED, 5
  field :GENERATION_CHANGED, 6
  field :PAYLOAD_TOO_LARGE, 7
  field :UNSUPPORTED_ENCODING, 8
  field :UNAUTHENTICATED, 20
  field :TOKEN_EXPIRED, 21
  field :SESSION_REVOKED, 22
  field :DEVICE_REVOKED, 23
  field :PERMISSION_DENIED, 24
  field :IDENTITY_PROOF_INVALID, 25
  field :POLICY_DENIED, 26
  field :NOT_FOUND, 40
  field :RATE_LIMITED, 60
  field :UNAVAILABLE, 61
  field :INTERNAL, 80
end

defmodule Lumia.V1.Error.DetailsEntry do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.Error.DetailsEntry",
    map: true,
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :key, 1, type: :string
  field :value, 2, type: :string
end

defmodule Lumia.V1.Error do
  @moduledoc false

  use Protobuf, full_name: "lumia.v1.Error", protoc_gen_elixir_version: "0.17.1", syntax: :proto3

  field :code, 1, type: Lumia.V1.ErrorCode, enum: true
  field :message, 2, type: :string
  field :details, 3, repeated: true, type: Lumia.V1.Error.DetailsEntry, map: true
  field :retry_after_seconds, 4, type: :uint32, json_name: "retryAfterSeconds"
  field :command_id, 5, type: :bytes, json_name: "commandId"
end

defmodule Lumia.V1.PageRequest do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.PageRequest",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :page_size, 1, type: :uint32, json_name: "pageSize"
  field :cursor, 2, type: :string
end

defmodule Lumia.V1.PageResponse do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.PageResponse",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :next_cursor, 1, type: :string, json_name: "nextCursor"
end

defmodule Lumia.V1.Cursor do
  @moduledoc false

  use Protobuf, full_name: "lumia.v1.Cursor", protoc_gen_elixir_version: "0.17.1", syntax: :proto3

  field :log_id, 1, type: :bytes, json_name: "logId"
  field :seq, 2, type: :uint64
end

defmodule Lumia.V1.ServerTime do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.ServerTime",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :at, 1, type: Google.Protobuf.Timestamp
end
