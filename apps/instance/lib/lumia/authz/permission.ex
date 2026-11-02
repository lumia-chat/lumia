defmodule Lumia.Authz.Permission do
  @moduledoc """
  The named permission flags and their storage bits.

  Bit index equals the proto `Permission` enum value in
  `proto/lumia/v1/chat.proto`, so a stored bitfield and a wire enum never
  need a translation table beyond this one. The API only ever sees names.
  """

  @bits %{
    administrator: 1,
    manage_server: 2,
    manage_roles: 3,
    manage_channels: 4,
    manage_members: 5,
    create_invites: 6,
    view_channel: 10,
    read_history: 11,
    send_messages: 12,
    send_in_threads: 13,
    attach_files: 14,
    add_reactions: 15,
    mention_everyone: 16,
    manage_messages: 17,
    start_calls: 20,
    start_server_dms: 21,
    manage_webhooks: 30,
    manage_bots: 31
  }

  @names Map.keys(@bits)
  @all Enum.reduce(@bits, 0, fn {_, bit}, acc -> Bitwise.bor(acc, Bitwise.bsl(1, bit)) end)

  @doc "Every permission name."
  def names, do: @names

  @doc "The bitfield with every permission set."
  def all, do: @all

  @doc "The bit for one permission."
  def bit(name), do: Bitwise.bsl(1, Map.fetch!(@bits, name))

  @doc "Names to bitfield."
  def to_bits(names), do: Enum.reduce(names, 0, &Bitwise.bor(&2, bit(&1)))

  @doc "Bitfield to sorted names. Unknown bits are ignored."
  def from_bits(bits) do
    @bits
    |> Enum.filter(fn {_, index} -> Bitwise.band(bits, Bitwise.bsl(1, index)) != 0 end)
    |> Enum.map(&elem(&1, 0))
    |> Enum.sort()
  end

  @doc "Whether `bits` includes `name`."
  def has?(bits, name), do: Bitwise.band(bits, bit(name)) != 0

  @doc "Whether every bit in `needed` is in `bits`."
  def includes?(bits, needed), do: Bitwise.band(bits, needed) == needed
end
