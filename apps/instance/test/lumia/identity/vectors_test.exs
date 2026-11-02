defmodule Lumia.Identity.VectorsTest do
  @moduledoc """
  Cross-implementation roster vectors (docs/plan.md, step 3 milestone 2).

  `proto/vectors/identity/rosters.json` holds rosters with the outcome the
  spec requires. This test reads it and checks `Lumia.Identity.Roster`
  against every vector; the Rust verifier in `apps/core` reads the same
  file. With `LUMIA_WRITE_VECTORS=1` it regenerates the file from the
  fixtures first, so the two implementations always see the same bytes.
  """

  use ExUnit.Case, async: true

  import Lumia.IdentityFixtures

  alias Lumia.Identity.Roster
  alias Lumia.V1

  @path Path.expand("../../../../../proto/vectors/identity/rosters.json", __DIR__)

  # A fixed clock so future-dated checks are reproducible.
  @now ~U[2026-11-01 12:00:00Z]

  defp at(seconds_from_now),
    do: Lumia.Proto.timestamp(DateTime.add(@now, seconds_from_now, :second))

  defp vectors do
    %{identity: identity, device: phone, roster: v1} = bootstrap_at(at(-1000))
    {:ok, held_v1} = Roster.verify(v1, now: @now)
    laptop = device("laptop")
    stranger = device("stranger")
    new_root = identity()

    [
      ok("v1_root_signed", v1, nil, %{
        version: 1,
        root_signed: true,
        devices: [phone.id],
        revoked: [],
        new_revocations: []
      }),
      ok(
        "v2_device_added_by_device",
        roster(identity, phone,
          version: 2,
          devices: [
            cert(identity, phone, :root, added_at: at(-1000)),
            cert(identity, laptop, phone, added_at: at(-500))
          ]
        ),
        nil,
        %{
          version: 2,
          root_signed: false,
          devices: [phone.id, laptop.id],
          revoked: [],
          new_revocations: []
        }
      ),
      ok(
        "v2_rotation",
        roster(identity, :root,
          version: 2,
          rotations: [rotation(identity, new_root.root_pub)],
          root_pub: new_root.root_pub,
          root_priv: new_root.root_priv,
          devices: [cert(identity, phone, :root, added_at: at(-1000))]
        ),
        nil,
        %{version: 2, root_signed: true, devices: [phone.id], revoked: [], new_revocations: []}
      ),
      ok(
        "cert_by_later_revoked_device",
        roster(identity, :root,
          version: 2,
          devices: [cert(identity, laptop, phone, added_at: at(-500))],
          revoked: [revocation(identity, phone, :root, added_at: at(-1000), revoked_at: at(-100))]
        ),
        nil,
        %{
          version: 2,
          root_signed: true,
          devices: [laptop.id],
          revoked: [phone.id],
          new_revocations: [phone.id]
        }
      ),
      ok(
        "monotonic_v2_superset_with_new_revocation",
        roster(identity, :root,
          version: 2,
          devices: [cert(identity, phone, :root, added_at: at(-1000))],
          revoked: [revocation(identity, laptop, :root, added_at: at(-900), revoked_at: at(-10))]
        ),
        v1,
        %{
          version: 2,
          root_signed: true,
          devices: [phone.id],
          revoked: [laptop.id],
          new_revocations: [laptop.id]
        }
      ),
      error(
        "identity_mismatch",
        roster(%{identity | identity_id: :crypto.strong_rand_bytes(32)}, :root,
          devices: [cert(%{identity | identity_id: :crypto.strong_rand_bytes(32)}, phone)]
        ),
        nil,
        :identity,
        :mismatch
      ),
      error(
        "cert_unknown_signer",
        roster(identity, :root,
          devices: [
            cert(identity, phone, :root, added_at: at(-1000)),
            cert(identity, laptop, stranger, added_at: at(-500))
          ]
        ),
        nil,
        :certs,
        :signer
      ),
      error(
        "cert_before_signer_added",
        roster(identity, :root,
          devices: [
            cert(identity, phone, :root, added_at: at(-100)),
            cert(identity, laptop, phone, added_at: at(-500))
          ]
        ),
        nil,
        :certs,
        :signer
      ),
      error(
        "cert_future_dated",
        roster(identity, :root, devices: [cert(identity, phone, :root, added_at: at(3600))]),
        nil,
        :certs,
        :future_dated
      ),
      error(
        "cert_by_device_revoked_before_it",
        roster(identity, :root,
          version: 2,
          devices: [cert(identity, laptop, phone, added_at: at(-50))],
          revoked: [revocation(identity, phone, :root, added_at: at(-1000), revoked_at: at(-100))]
        ),
        nil,
        :certs,
        :signer
      ),
      error(
        "mirror_mismatch",
        roster(identity, :root,
          devices: [%{cert(identity, phone, :root, added_at: at(-1000)) | kind: :WEB}]
        ),
        nil,
        :parse,
        :mirror_mismatch
      ),
      error(
        "self_revocation",
        roster(identity, :root,
          revoked: [revocation(identity, phone, phone, added_at: at(-1000))]
        ),
        nil,
        :revocations,
        :signer
      ),
      error(
        "roster_signed_by_stranger",
        roster(identity, stranger, devices: [cert(identity, phone, :root, added_at: at(-1000))]),
        nil,
        :signature,
        :none_valid
      ),
      error(
        "broken_rotation_chain",
        roster(identity, :root,
          version: 2,
          rotations: [rotation(identity, identity().root_pub)],
          root_pub: new_root.root_pub,
          root_priv: new_root.root_priv,
          devices: [cert(identity, phone, :root, added_at: at(-1000))]
        ),
        nil,
        :chain,
        :head_mismatch
      ),
      error(
        "monotonic_same_version",
        roster(identity, :root,
          version: 1,
          devices: [cert(identity, phone, :root, added_at: at(-1000))]
        ),
        v1,
        :monotonic,
        :version
      ),
      error(
        "monotonic_device_dropped",
        roster(identity, :root, version: 2, devices: []),
        v1,
        :monotonic,
        :device_dropped
      ),
      error(
        "undecodable",
        %V1.SignedRoster{body: "garbage", signatures: []},
        nil,
        :parse,
        :undecodable
      )
    ]
    |> then(fn list -> {list, held_v1} end)
  end

  defp bootstrap_at(added_at) do
    identity = identity()
    phone = device("phone")

    %{
      identity: identity,
      device: phone,
      roster: roster(identity, :root, devices: [cert(identity, phone, :root, added_at: added_at)])
    }
  end

  defp ok(name, roster, held, expect) do
    %{
      "name" => name,
      "roster" => b64(V1.SignedRoster.encode(roster)),
      "held" => held && b64(V1.SignedRoster.encode(held)),
      "now" => DateTime.to_unix(@now),
      "expect" => %{
        "ok" => %{
          "version" => expect.version,
          "root_signed" => expect.root_signed,
          "devices" => Enum.map(expect.devices, &hex/1) |> Enum.sort(),
          "revoked" => Enum.map(expect.revoked, &hex/1) |> Enum.sort(),
          "new_revocations" => Enum.map(expect.new_revocations, &hex/1) |> Enum.sort()
        }
      }
    }
  end

  defp error(name, roster, held, step, reason) do
    %{
      "name" => name,
      "roster" => b64(V1.SignedRoster.encode(roster)),
      "held" => held && b64(V1.SignedRoster.encode(held)),
      "now" => DateTime.to_unix(@now),
      "expect" => %{"error" => %{"step" => to_string(step), "reason" => to_string(reason)}}
    }
  end

  defp b64(bytes), do: Base.encode64(bytes)
  defp hex(bytes), do: Base.encode16(bytes, case: :lower)

  test "the committed vectors agree with Lumia.Identity.Roster" do
    if System.get_env("LUMIA_WRITE_VECTORS") == "1" do
      {list, _held} = vectors()
      File.mkdir_p!(Path.dirname(@path))
      File.write!(@path, Jason.encode!(%{"vectors" => list}, pretty: true) <> "\n")
    end

    assert File.exists?(@path), "no vectors; run with LUMIA_WRITE_VECTORS=1"
    %{"vectors" => list} = @path |> File.read!() |> Jason.decode!()
    assert length(list) >= 10

    for vector <- list do
      roster = vector["roster"] |> Base.decode64!() |> V1.SignedRoster.decode()
      now = DateTime.from_unix!(vector["now"])

      held =
        case vector["held"] do
          nil -> nil
          b -> elem(Roster.verify(b |> Base.decode64!() |> V1.SignedRoster.decode(), now: now), 1)
        end

      result = Roster.verify(roster, now: now, held: held)

      case vector["expect"] do
        %{"ok" => expect} ->
          assert {:ok, verified} = result, "#{vector["name"]}: #{inspect(result)}"
          assert verified.version == expect["version"], vector["name"]
          assert verified.root_signed? == expect["root_signed"], vector["name"]

          assert verified.devices |> Map.keys() |> Enum.map(&hex/1) |> Enum.sort() ==
                   expect["devices"],
                 vector["name"]

          assert verified.revoked |> Map.keys() |> Enum.map(&hex/1) |> Enum.sort() ==
                   expect["revoked"],
                 vector["name"]

          assert verified.new_revocations |> Enum.map(&hex/1) |> Enum.sort() ==
                   expect["new_revocations"],
                 vector["name"]

        %{"error" => %{"step" => step, "reason" => reason}} ->
          assert {:error, {got_step, got_reason}} = result,
                 "#{vector["name"]}: #{inspect(result)}"

          assert {to_string(got_step), to_string(got_reason)} == {step, reason}, vector["name"]
      end
    end
  end
end
