defmodule Lumia.Cards.Address do
  @moduledoc """
  Which addresses the card fetcher may connect to (specs/media.md 6.3):
  none that is private, loopback, link-local, unique-local, multicast,
  unspecified, broadcast, CGNAT, documentation or benchmarking, IPv4 and
  IPv6 alike, an IPv6 address carrying an IPv4 one (mapped, compatible,
  NAT64, 6to4) judged by the IPv4 it carries. The same rules as the core's
  `lumia_core::cards::address_allowed`, checked against the same vectors.
  """

  import Bitwise

  @doc "Whether the fetcher may connect to `ip`, an `:inet` address tuple."
  def allowed?({a, b, c, d} = ip) when is_integer(a) and is_integer(d) do
    _ = {b, c}
    not Enum.any?(v4_refused(), fn {net, bits} -> in_v4?(ip, net, bits) end)
  end

  def allowed?({0, 0, 0, 0, 0, 0xFFFF, hi, lo}), do: allowed?(v4(hi, lo))
  def allowed?({0, 0, 0, 0, 0, 0, hi, lo}) when hi != 0 or lo > 1, do: allowed?(v4(hi, lo))
  def allowed?({0x64, 0xFF9B, 0, 0, 0, 0, hi, lo}), do: allowed?(v4(hi, lo))
  def allowed?({0x2002, hi, lo, _, _, _, _, _}), do: allowed?(v4(hi, lo))

  def allowed?({_, _, _, _, _, _, _, _} = ip),
    do: not Enum.any?(v6_refused(), fn {net, bits} -> in_v6?(ip, net, bits) end)

  def allowed?(_), do: false

  defp v4(hi, lo), do: {hi >>> 8, hi &&& 0xFF, lo >>> 8, lo &&& 0xFF}

  defp v4_refused do
    [
      {{0, 0, 0, 0}, 8},
      {{10, 0, 0, 0}, 8},
      {{100, 64, 0, 0}, 10},
      {{127, 0, 0, 0}, 8},
      {{169, 254, 0, 0}, 16},
      {{172, 16, 0, 0}, 12},
      {{192, 0, 0, 0}, 24},
      {{192, 0, 2, 0}, 24},
      {{192, 88, 99, 0}, 24},
      {{192, 168, 0, 0}, 16},
      {{198, 18, 0, 0}, 15},
      {{198, 51, 100, 0}, 24},
      {{203, 0, 113, 0}, 24},
      {{224, 0, 0, 0}, 4},
      {{240, 0, 0, 0}, 4}
    ]
  end

  defp v6_refused do
    [
      {{0, 0, 0, 0, 0, 0, 0, 0}, 128},
      {{0, 0, 0, 0, 0, 0, 0, 1}, 128},
      {{0x100, 0, 0, 0, 0, 0, 0, 0}, 64},
      {{0x2001, 0, 0, 0, 0, 0, 0, 0}, 23},
      {{0x2001, 0xDB8, 0, 0, 0, 0, 0, 0}, 32},
      {{0xFC00, 0, 0, 0, 0, 0, 0, 0}, 7},
      {{0xFE80, 0, 0, 0, 0, 0, 0, 0}, 10},
      {{0xFEC0, 0, 0, 0, 0, 0, 0, 0}, 10},
      {{0xFF00, 0, 0, 0, 0, 0, 0, 0}, 8}
    ]
  end

  defp in_v4?(ip, net, bits),
    do: prefix(to_int(ip, 8), 32, bits) == prefix(to_int(net, 8), 32, bits)

  defp in_v6?(ip, net, bits),
    do: prefix(to_int(ip, 16), 128, bits) == prefix(to_int(net, 16), 128, bits)

  defp to_int(tuple, width),
    do: tuple |> Tuple.to_list() |> Enum.reduce(0, fn part, acc -> (acc <<< width) + part end)

  defp prefix(int, size, bits), do: int >>> (size - bits)
end
