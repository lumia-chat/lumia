defmodule Lumia.Proto do
  @moduledoc """
  Conversions between Ecto schemas and the generated `Lumia.V1` wire
  structs, plus the scalar rules from docs/protocol.md section 2: UUIDs are
  16 raw bytes on the wire and strings in Ecto; timestamps are
  `Google.Protobuf.Timestamp`.
  """

  alias Google.Protobuf.Timestamp
  alias Lumia.V1

  # ---- Scalars -----------------------------------------------------------------

  @doc "String UUID to 16 bytes. nil and empty map to the proto default (empty)."
  def uuid(nil), do: ""
  def uuid(""), do: ""

  def uuid(<<_::288>> = string) do
    {:ok, bin} = Ecto.UUID.dump(string)
    bin
  end

  @doc """
  16 bytes to string UUID. Empty maps to nil, and so does anything that is
  not 16 bytes: an id a client got wrong names nothing, and callers treat
  nil as absent or invalid rather than crashing on it.
  """
  def from_uuid(<<_::128>> = bin) do
    {:ok, string} = Ecto.UUID.load(bin)
    string
  end

  def from_uuid(_other), do: nil

  @doc "DateTime to Timestamp. nil stays nil."
  def timestamp(nil), do: nil

  def timestamp(%DateTime{} = dt) do
    %Timestamp{seconds: DateTime.to_unix(dt, :second), nanos: elem(dt.microsecond, 0) * 1_000}
  end

  @doc "Timestamp to DateTime with microsecond precision. nil stays nil."
  def from_timestamp(nil), do: nil

  def from_timestamp(%Timestamp{seconds: s, nanos: n}) do
    DateTime.from_unix!(s * 1_000_000 + div(n, 1_000), :microsecond)
  end

  # ---- Chat shapes -------------------------------------------------------------

  def server(%Lumia.Servers.Server{} = s) do
    %V1.Server{
      server_id: uuid(s.id),
      name: s.name,
      icon_url: s.icon_url || "",
      owner_account_id: uuid(s.owner_account_id),
      created_at: timestamp(s.inserted_at),
      retention_days: s.retention_days || 0
    }
  end

  def invite(%Lumia.Invites.Invite{} = i) do
    %V1.Invite{
      code: i.code,
      server_id: uuid(i.server_id),
      channel_id: uuid(i.channel_id),
      created_by: uuid(i.created_by),
      expires_at: timestamp(i.expires_at),
      max_uses: i.max_uses,
      uses: i.uses,
      as_guest: i.as_guest,
      guest_expires_at: nil,
      email: i.email || ""
    }
  end

  @doc """
  A membership on the wire. `permissions`, the member's resolved
  server-level bitfield, is given only for the caller's own memberships in
  its account snapshot (architecture 6.7).
  """
  def member(%Lumia.Servers.Member{} = m, role_ids \\ [], permissions \\ 0) do
    %V1.Member{
      server_id: uuid(m.server_id),
      account_id: uuid(m.account_id),
      kind: membership_kind(m.kind),
      role_ids: Enum.map(role_ids, &uuid/1),
      nickname: m.nickname || "",
      joined_at: timestamp(m.joined_at),
      expires_at: timestamp(m.expires_at),
      permissions: permissions |> Lumia.Authz.Permission.from_bits() |> Enum.map(&permission/1),
      channel_adds_by_invitation: m.channel_adds_by_invitation,
      server_dms_off: m.server_dms_off
    }
  end

  @doc """
  A channel on the wire. `member_count` is given only where a listing asks
  for it (Browse, architecture 6.8).
  """
  def channel(%Lumia.Events.Channel{} = c, member_count \\ 0) do
    %V1.Channel{
      channel_id: uuid(c.id),
      kind: channel_kind(c.kind),
      server_id: uuid(c.server_id),
      name: c.name || "",
      topic: c.topic || "",
      position: c.position,
      last_seq: c.last_seq,
      acl_version: c.acl_version,
      created_at: timestamp(c.inserted_at),
      participant_account_ids: participants(c),
      host: if(Lumia.Dms.personal_kind?(c.kind), do: Lumia.Identity.host(), else: ""),
      private: c.private,
      is_default: c.is_default,
      member_count: member_count
    }
  end

  # DMs carry their participants: a personal one's so a client can fetch
  # rosters, a server one's so it can name the conversation.
  defp participants(%Lumia.Events.Channel{kind: "server_dm", id: id}),
    do: Enum.map(Lumia.Channels.member_ids(id), &uuid/1)

  defp participants(%Lumia.Events.Channel{kind: kind, id: id}) do
    if Lumia.Dms.personal_kind?(kind),
      do: Enum.map(Lumia.Dms.participants(id), &uuid/1),
      else: []
  end

  @doc """
  A materialized message on the wire, with its reactions as the reader sees
  them (`reactions`, `Lumia.V1.ReactionCount`s, which `Lumia.Snapshots`
  makes for the reader).
  """
  def message(%Lumia.Events.Message{} = m, reactions \\ []) do
    %V1.Message{
      message_id: uuid(m.id),
      channel_id: uuid(m.channel_id),
      seq: m.seq,
      author_account_id: uuid(m.author_account_id),
      parent_id: uuid(m.parent_id),
      body: m.body || "",
      created_at: timestamp(m.created_at),
      edited_at: timestamp(m.edited_at),
      deleted_at: timestamp(m.deleted_at),
      notice: notice(m.notice, m.notice_account_id),
      mentions: Enum.flat_map(m.mention_spans || [], &mention/1),
      reply_to: quote_reply(m),
      reactions: reactions
    }
  end

  defp quote_reply(%Lumia.Events.Message{reply_to_id: nil}), do: nil

  defp quote_reply(%Lumia.Events.Message{} = m) do
    %V1.QuoteReply{
      message_id: uuid(m.reply_to_id),
      author_account_id: uuid(m.reply_to_author_id),
      text: m.reply_text || "",
      original_deleted: m.reply_deleted,
      # Sent without notifying the author quoted: they are not among those it notifies.
      silent: m.reply_to_author_id not in (m.mentioned_account_ids || [])
    }
  end

  @doc "A mention as a message row keeps it: its target and where it is in the body."
  def mention_span(%V1.Mention{target: {:account_id, id}, start: s, end: e}) do
    case from_uuid(id) do
      nil -> []
      account_id -> [%{"account_id" => account_id, "start" => s, "end" => e}]
    end
  end

  def mention_span(%V1.Mention{target: {:everyone, true}, start: s, end: e}),
    do: [%{"everyone" => true, "start" => s, "end" => e}]

  def mention_span(_), do: []

  defp mention(%{"account_id" => id, "start" => s, "end" => e}),
    do: [%V1.Mention{target: {:account_id, uuid(id)}, start: s, end: e}]

  defp mention(%{"everyone" => true, "start" => s, "end" => e}),
    do: [%V1.Mention{target: {:everyone, true}, start: s, end: e}]

  defp mention(_), do: []

  @doc "A channel notice (architecture 6.8) from its kind as kept and who it is about."
  def notice(nil, _account_id), do: nil

  def notice(kind, account_id) do
    %V1.ChannelNotice{kind: notice_kind(kind), account_id: uuid(account_id)}
  end

  defp notice_kind("joined"), do: :JOINED
  defp notice_kind("added"), do: :ADDED
  defp notice_kind("left"), do: :LEFT
  defp notice_kind("removed"), do: :REMOVED

  def dm_request(%Lumia.Dms.Request{} = r) do
    %V1.DmRequest{
      request_id: uuid(r.id),
      from_account_id: uuid(r.from_account_id),
      to_account_id: uuid(r.to_account_id),
      state: dm_request_state(r.state),
      created_at: timestamp(r.created_at)
    }
  end

  def dm_request_state("pending"), do: :PENDING
  def dm_request_state("accepted"), do: :ACCEPTED
  def dm_request_state("declined"), do: :DECLINED
  def dm_request_state("blocked"), do: :BLOCKED

  @doc "An account; with `own: true`, the caller's, carrying its signed profile."
  def account(%Lumia.Accounts.Account{} = a, opts \\ []) do
    %V1.Account{
      account_id: uuid(a.id),
      identity_id: a.identity_id || "",
      kind: account_kind(a.kind),
      handle: a.handle,
      display_name: a.display_name || "",
      avatar_url: a.avatar_url || "",
      pronouns: a.pronouns || "",
      bio: a.bio || "",
      created_at: timestamp(a.inserted_at),
      profile:
        if(Keyword.get(opts, :own, false) and is_binary(a.profile),
          do: V1.SignedProfile.decode(a.profile),
          else: nil
        )
    }
  end

  def role(%Lumia.Servers.Role{} = r) do
    %V1.Role{
      role_id: uuid(r.id),
      server_id: uuid(r.server_id),
      name: r.name,
      position: r.position,
      permissions: r.permissions |> Lumia.Authz.Permission.from_bits() |> Enum.map(&permission/1),
      color: r.color || "",
      is_everyone: r.is_everyone
    }
  end

  @doc "Permission name atom to the wire enum (`:view_channel` → `:VIEW_CHANNEL`)."
  def permission(name),
    do:
      name |> Atom.to_string() |> String.upcase() |> V1.Permission.value() |> V1.Permission.key()

  @doc "Wire enum to the permission name atom. Unknown values are dropped by the caller."
  def from_permission(enum),
    do: enum |> Atom.to_string() |> String.downcase() |> String.to_existing_atom()

  def account_kind("human"), do: :ACCOUNT_HUMAN
  def account_kind("bot"), do: :ACCOUNT_BOT

  def membership_kind("member"), do: :MEMBER
  def membership_kind("guest"), do: :GUEST

  def channel_kind("text"), do: :TEXT
  def channel_kind("server_dm"), do: :SERVER_DM
  def channel_kind("server_group_dm"), do: :SERVER_GROUP_DM
  def channel_kind("personal_dm"), do: :PERSONAL_DM
  def channel_kind("personal_group_dm"), do: :PERSONAL_GROUP_DM

  def leave_reason("left"), do: :LEFT
  def leave_reason("removed"), do: :REMOVED
  def leave_reason("guest_expired"), do: :GUEST_EXPIRED
  def leave_reason("account_deleted"), do: :ACCOUNT_DELETED

  @doc "The event kind string stored in the events table for a payload tag."
  # An encrypted payload is named for what it carries, so what is unread in
  # a personal conversation counts people's messages and not its commits.
  def kind({:encrypted, %V1.EncryptedPayload{kind: :APPLICATION}}), do: "encrypted"
  def kind({:encrypted, %V1.EncryptedPayload{kind: :CONTROL}}), do: "encrypted_control"
  def kind({:encrypted, %V1.EncryptedPayload{kind: :WELCOME}}), do: "encrypted_welcome"
  def kind({tag, _struct}) when is_atom(tag), do: Atom.to_string(tag)
end
