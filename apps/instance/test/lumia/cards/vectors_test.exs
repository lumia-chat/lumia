defmodule Lumia.Cards.VectorsTest do
  @moduledoc """
  The card vectors the core's `lumia_core::cards` reads too
  (`proto/vectors/cards`): what a page's head gives, and which addresses
  the fetcher may reach. The instance builds server cards and the core
  personal ones, so the two must agree on every case.
  """

  use ExUnit.Case, async: true

  alias Lumia.Cards.Address
  alias Lumia.Cards.Head

  @dir Path.expand("../../../../../proto/vectors/cards", __DIR__)

  defp vectors(file),
    do: Path.join(@dir, file) |> File.read!() |> Jason.decode!() |> Map.fetch!("vectors")

  defp expected(nil), do: nil
  defp expected(map), do: Map.new(map, fn {k, value} -> {String.to_existing_atom(k), value} end)

  test "every page gives the card the vectors say" do
    failures =
      for v <- vectors("cases.json"),
          got = Head.parse(File.read!(Path.join(@dir, v["file"])), v["page_url"]),
          expected = expected(v["expect"]),
          got != expected,
          do: {v["name"], v["note"], got, expected}

    assert failures == []
  end

  test "every address is allowed or refused as the vectors say" do
    failures =
      for v <- vectors("addresses.json"),
          {:ok, ip} = :inet.parse_address(String.to_charlist(v["ip"])),
          Address.allowed?(ip) != v["allowed"],
          do: {v["ip"], v["why"]}

    assert failures == []
  end
end
