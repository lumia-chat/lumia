defmodule Lumia.Cards.Address do
  @moduledoc """
  Which addresses the card fetcher may connect to (specs/media.md 6.3). A
  port of the core's `lumia_core::cards::address_allowed`, checked against
  the same vectors (`proto/vectors/cards/addresses.json`): no private,
  loopback, link-local, CGNAT, documentation, benchmarking, multicast or
  reserved IPv4; IPv6 only within global unicast (2000::/3) and outside
  its special ranges; an IPv6 address that carries an IPv4 one (mapped,
  compatible, translated, NAT64's well-known prefix, 6to4) judged by the
  IPv4 it carries.
  """

  import Bitwise

  @denied_v4 [
    {0x0000_0000, 8},
    {0x0A00_0000, 8},
    {0x6440_0000, 10},
    {0x7F00_0000, 8},
    {0xA9FE_0000, 16},
    {0xAC10_0000, 12},
    {0xC000_0000, 24},
    {0xC000_0200, 24},
    {0xC058_6300, 24},
    {0xC0A8_0000, 16},
    {0xC612_0000, 15},
    {0xC633_6400, 24},
    {0xCB00_7100, 24},
    {0xE000_0000, 4},
    {0xF000_0000, 4}
  ]

  @denied_v6 [
    # Teredo (its IPv4 is obscured), benchmarking, ORCHID and ORCHIDv2,
    # documentation, and documentation again (RFC 9637).
    {0x2001_0000 <<< 96, 32},
    {0x2001_0002 <<< 96, 48},
    {0x2001_0010 <<< 96, 28},
    {0x2001_0020 <<< 96, 28},
    {0x2001_0DB8 <<< 96, 32},
    {0x3FFF_0000 <<< 96, 20}
  ]

  @doc "Whether the fetcher may connect to `ip`, an `:inet` address tuple."
  def allowed?({_, _, _, _} = ip), do: v4_allowed?(to_int(ip, 8))

  def allowed?({_, _, _, _, _, _, _, _} = ip) do
    n = to_int(ip, 16)

    case embedded_v4(n) do
      nil -> n >>> 125 == 0b001 and not denied?(n, 128, @denied_v6)
      v4 -> v4_allowed?(v4)
    end
  end

  def allowed?(_), do: false

  defp v4_allowed?(n), do: not denied?(n, 32, @denied_v4)

  defp denied?(n, size, ranges),
    do: Enum.any?(ranges, fn {net, len} -> bxor(n, net) >>> (size - len) == 0 end)

  defp embedded_v4(n) do
    cond do
      (n >>> 32) in [0, 0xFFFF, 0xFFFF_0000_0000, 0x0064_FF9B_0000_0000_0000_0000] ->
        n &&& 0xFFFF_FFFF

      n >>> 112 == 0x2002 ->
        n >>> 80 &&& 0xFFFF_FFFF

      true ->
        nil
    end
  end

  defp to_int(tuple, width),
    do: tuple |> Tuple.to_list() |> Enum.reduce(0, fn part, acc -> (acc <<< width) + part end)
end
