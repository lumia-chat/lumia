defmodule Lumia.V1.Command do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.Command",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  oneof(:op, 0)

  field :command_id, 1, type: :bytes, json_name: "commandId"
  field :channel_id, 2, type: :bytes, json_name: "channelId"
  field :send_message, 10, type: Lumia.V1.SendMessage, json_name: "sendMessage", oneof: 0
  field :edit_message, 11, type: Lumia.V1.EditMessage, json_name: "editMessage", oneof: 0
  field :delete_message, 12, type: Lumia.V1.DeleteMessage, json_name: "deleteMessage", oneof: 0
  field :add_reaction, 13, type: Lumia.V1.AddReaction, json_name: "addReaction", oneof: 0
  field :remove_reaction, 14, type: Lumia.V1.RemoveReaction, json_name: "removeReaction", oneof: 0
  field :pin_message, 15, type: Lumia.V1.PinMessage, json_name: "pinMessage", oneof: 0
  field :unpin_message, 16, type: Lumia.V1.UnpinMessage, json_name: "unpinMessage", oneof: 0
  field :send_encrypted, 20, type: Lumia.V1.SendEncrypted, json_name: "sendEncrypted", oneof: 0
end

defmodule Lumia.V1.SendMessage do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.SendMessage",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :body, 1, type: :string
  field :mentions, 2, repeated: true, type: Lumia.V1.Mention
  field :attachment_ids, 3, repeated: true, type: :bytes, json_name: "attachmentIds"
  field :parent_id, 4, type: :bytes, json_name: "parentId"
  field :also_send_to_channel, 5, type: :bool, json_name: "alsoSendToChannel"
  field :reply_to, 6, type: Lumia.V1.QuoteReply, json_name: "replyTo"
end

defmodule Lumia.V1.EditMessage do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.EditMessage",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :message_id, 1, type: :bytes, json_name: "messageId"
  field :body, 2, type: :string
  field :mentions, 3, repeated: true, type: Lumia.V1.Mention
end

defmodule Lumia.V1.DeleteMessage do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.DeleteMessage",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :message_id, 1, type: :bytes, json_name: "messageId"
end

defmodule Lumia.V1.AddReaction do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.AddReaction",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :message_id, 1, type: :bytes, json_name: "messageId"
  field :reaction, 2, type: :string
end

defmodule Lumia.V1.RemoveReaction do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.RemoveReaction",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :message_id, 1, type: :bytes, json_name: "messageId"
  field :reaction, 2, type: :string
end

defmodule Lumia.V1.PinMessage do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.PinMessage",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :message_id, 1, type: :bytes, json_name: "messageId"
end

defmodule Lumia.V1.UnpinMessage do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.UnpinMessage",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :message_id, 1, type: :bytes, json_name: "messageId"
end

defmodule Lumia.V1.SendEncrypted do
  @moduledoc false

  use Protobuf,
    full_name: "lumia.v1.SendEncrypted",
    protoc_gen_elixir_version: "0.17.1",
    syntax: :proto3

  field :payload, 1, type: Lumia.V1.EncryptedPayload
end

defmodule Lumia.V1.Ack do
  @moduledoc false

  use Protobuf, full_name: "lumia.v1.Ack", protoc_gen_elixir_version: "0.17.1", syntax: :proto3

  field :command_id, 1, type: :bytes, json_name: "commandId"
  field :channel_id, 2, type: :bytes, json_name: "channelId"
  field :seq, 3, type: :uint64
  field :event_id, 4, type: :bytes, json_name: "eventId"
  field :mirror_seq, 5, type: :uint64, json_name: "mirrorSeq"
  field :mirror_event_id, 6, type: :bytes, json_name: "mirrorEventId"
end
