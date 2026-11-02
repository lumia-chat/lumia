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
      created_at: timestamp(s.inserted_at)
    }
  end

  def member(%Lumia.Servers.Member{} = m, role_ids \\ []) do
    %V1.Member{
      server_id: uuid(m.server_id),
      account_id: uuid(m.account_id),
      kind: membership_kind(m.kind),
      role_ids: Enum.map(role_ids, &uuid/1),
      nickname: m.nickname || "",
      joined_at: timestamp(m.joined_at),
      expires_at: timestamp(m.expires_at)
    }
  end

  def channel(%Lumia.Events.Channel{} = c) do
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
      host: if(Lumia.Dms.personal_kind?(c.kind), do: Lumia.Identity.host(), else: "")
    }
  end

  # Personal DMs carry their participants so a client can fetch rosters.
  defp participants(%Lumia.Events.Channel{kind: kind, id: id}) do
    if Lumia.Dms.personal_kind?(kind),
      do: Enum.map(Lumia.Dms.participants(id), &uuid/1),
      else: []
  end

  def message(%Lumia.Events.Message{} = m) do
    %V1.Message{
      message_id: uuid(m.id),
      channel_id: uuid(m.channel_id),
      seq: m.seq,
      author_account_id: uuid(m.author_account_id),
      parent_id: uuid(m.parent_id),
      body: m.body || "",
      created_at: timestamp(m.created_at),
      edited_at: timestamp(m.edited_at),
      deleted_at: timestamp(m.deleted_at)
    }
  end

  def account(%Lumia.Accounts.Account{} = a) do
    %V1.Account{
      account_id: uuid(a.id),
      identity_id: a.identity_id || "",
      kind: account_kind(a.kind),
      handle: a.handle,
      display_name: a.display_name || "",
      avatar_url: a.avatar_url || "",
      created_at: timestamp(a.inserted_at)
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
    do: name |> Atom.to_string() |> String.upcase() |> String.to_existing_atom()

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
  def kind({tag, _struct}) when is_atom(tag), do: Atom.to_string(tag)
end
