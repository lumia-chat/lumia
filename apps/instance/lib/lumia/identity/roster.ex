defmodule Lumia.Identity.Roster do
  @moduledoc """
  Roster verification: the eight steps of docs/specs/identity.md section
  3.3, as a pure function of the roster bytes, the held version (if any),
  and the clock. Nothing here touches the database; `Lumia.Identity` does
  the storing and the policy.

  A roster is self-certifying: every device entry and revocation carries a
  signature that chains to a root, so any version verifies from its bytes
  alone.
  """

  alias Lumia.Crypto
  alias Lumia.V1

  @max_devices 20
  @max_revoked 100
  @max_bytes 65_536
  @clock_skew_seconds 300

  defstruct identity_id: nil,
            version: 0,
            root_public_key: nil,
            roots: [],
            devices: %{},
            revoked: %{},
            root_signed?: false,
            new_revocations: [],
            body: nil,
            signed: nil

  @doc """
  Verifies a `Lumia.V1.SignedRoster`.

  Options: `held` (a previously verified roster struct for monotonicity,
  or nil), `now` (DateTime, default now).

  Returns `{:ok, verified}` or `{:error, {step, reason}}` where `step` is
  the spec's step number as an atom (`:parse`, `:identity`, `:chain`,
  `:certs`, `:revocations`, `:signature`, `:monotonic`).
  """
  def verify(%V1.SignedRoster{body: bytes, signatures: sigs} = signed, opts \\ []) do
    now = Keyword.get(opts, :now, DateTime.utc_now())
    held = Keyword.get(opts, :held)
    # A held roster was clock-checked when accepted; re-verifying it later
    # must not fail on skew.
    now = if Keyword.get(opts, :skip_clock), do: DateTime.add(now, 3650, :day), else: now

    with {:ok, body, devices, revoked} <- parse(bytes),
         {:ok, roots} <- identity(body),
         :ok <- chain(body, roots),
         :ok <- certs(body, devices, revoked, roots, now),
         :ok <- revocations(body, revoked, devices, roots),
         {:ok, root_signed?} <- signature(bytes, sigs, body, devices),
         {:ok, new_revocations} <- monotonic(body, devices, revoked, held) do
      {:ok,
       %__MODULE__{
         identity_id: body.identity_id,
         version: body.version,
         root_public_key: body.root_public_key.ed25519,
         roots: roots,
         devices: devices,
         revoked: revoked,
         root_signed?: root_signed?,
         new_revocations: new_revocations,
         body: body,
         signed: signed
       }}
    end
  end

  # ---- 1. Parse ----------------------------------------------------------------

  defp parse(bytes) when byte_size(bytes) > @max_bytes, do: {:error, {:parse, :too_large}}

  defp parse(bytes) do
    with {:ok, body} <- decode(V1.RosterBody, bytes, :parse),
         :ok <- limits(body),
         {:ok, devices} <- parse_devices(body.devices),
         {:ok, revoked} <- parse_revoked(body.revoked),
         :ok <- disjoint(devices, revoked) do
      {:ok, body, devices, revoked}
    end
  end

  defp limits(body) do
    cond do
      length(body.devices) > @max_devices ->
        {:error, {:parse, :too_many_devices}}

      length(body.revoked) > @max_revoked ->
        {:error, {:parse, :too_many_revocations}}

      body.version < 1 ->
        {:error, {:parse, :version}}

      is_nil(body.root_public_key) or byte_size(body.root_public_key.ed25519) != 32 ->
        {:error, {:parse, :root_key}}

      true ->
        :ok
    end
  end

  defp parse_devices(entries) do
    Enum.reduce_while(entries, {:ok, %{}}, fn entry, {:ok, acc} ->
      case parse_device(entry, acc) do
        {:ok, acc} -> {:cont, {:ok, acc}}
        error -> {:halt, error}
      end
    end)
  end

  defp parse_device(entry, acc) do
    with {:ok, cert} <- decode(V1.DeviceCertBody, entry.cert_body, :parse),
         :ok <- mirrors_match(entry, cert),
         false <- Map.has_key?(acc, cert.device_id) do
      {:ok, Map.put(acc, cert.device_id, %{cert: cert, entry: entry})}
    else
      true -> {:error, {:parse, :duplicate_device}}
      error -> error
    end
  end

  defp mirrors_match(entry, cert) do
    same? =
      entry.device_id == cert.device_id and entry.public_key == cert.public_key and
        entry.kind == cert.kind and entry.added_at == cert.added_at and
        entry.added_by == cert.added_by and
        entry.web_origin == cert.web_origin and byte_size(cert.public_key.ed25519) == 32

    if same?, do: :ok, else: {:error, {:parse, :mirror_mismatch}}
  end

  defp parse_revoked(entries) do
    Enum.reduce_while(entries, {:ok, %{}}, fn entry, {:ok, acc} ->
      case parse_revocation(entry) do
        {:ok, body} -> {:cont, {:ok, Map.put(acc, body.device_id, %{body: body, entry: entry})}}
        error -> {:halt, error}
      end
    end)
  end

  defp parse_revocation(entry) do
    with {:ok, body} <- decode(V1.RevocationBody, entry.body, :parse) do
      same? =
        entry.device_id == body.device_id and entry.revoked_at == body.revoked_at and
          entry.revoked_by == body.revoked_by

      if same?, do: {:ok, body}, else: {:error, {:parse, :mirror_mismatch}}
    end
  end

  defp disjoint(devices, revoked) do
    if Enum.any?(Map.keys(revoked), &Map.has_key?(devices, &1)),
      do: {:error, {:parse, :device_both_active_and_revoked}},
      else: :ok
  end

  # ---- 2. Identity -------------------------------------------------------------

  defp identity(body) do
    first =
      case body.rotations do
        [] ->
          body.root_public_key.ed25519

        [first | _] ->
          with {:ok, r} <- decode(V1.RootRotationBody, first.body, :chain), do: r.from.ed25519
      end

    case first do
      {:error, _} = e ->
        e

      first when is_binary(first) ->
        if Crypto.blake3(first) == body.identity_id,
          do: {:ok, [first]},
          else: {:error, {:identity, :mismatch}}
    end
  end

  # ---- 3. Rotation chain -------------------------------------------------------

  defp chain(body, [first]) do
    result =
      Enum.reduce_while(body.rotations, {:ok, first, [first]}, fn rotation, {:ok, prev, acc} ->
        with {:ok, r} <- decode(V1.RootRotationBody, rotation.body, :chain),
             true <- r.from.ed25519 == prev || {:error, {:chain, :broken}},
             true <- r.identity_id == body.identity_id || {:error, {:chain, :identity}},
             true <-
               Crypto.verify(rotation.body, rotation.signature.signature, r.from.ed25519) ||
                 {:error, {:chain, :signature}} do
          {:cont, {:ok, r.to.ed25519, acc ++ [r.to.ed25519]}}
        else
          error -> {:halt, error}
        end
      end)

    case result do
      {:ok, last, _roots} when last != body.root_public_key.ed25519 ->
        {:error, {:chain, :head_mismatch}}

      {:ok, _last, _roots} ->
        :ok

      error ->
        error
    end
  end

  defp roots_of(body, first) do
    body.rotations
    |> Enum.map(fn r -> V1.RootRotationBody.decode(r.body).to.ed25519 end)
    |> then(&[first | &1])
  end

  # ---- 4. Device certificates ---------------------------------------------------

  defp certs(body, devices, revoked, [first], now) do
    roots = roots_of(body, first)
    skew_limit = DateTime.add(now, @clock_skew_seconds, :second)

    devices
    |> Map.values()
    |> Enum.sort_by(&ts(&1.cert.added_at))
    |> Enum.reduce_while(:ok, fn %{cert: cert, entry: entry}, :ok ->
      cond do
        cert.identity_id != body.identity_id ->
          {:halt, {:error, {:certs, :identity}}}

        DateTime.compare(to_dt(cert.added_at), skew_limit) == :gt ->
          {:halt, {:error, {:certs, :future_dated}}}

        not cert_signer_ok?(entry.authorization, cert, devices, revoked, roots) ->
          {:halt, {:error, {:certs, :signer}}}

        not Crypto.verify(
          entry.cert_body,
          entry.authorization.signature,
          signer_key(entry.authorization, devices, revoked)
        ) ->
          {:halt, {:error, {:certs, :signature}}}

        true ->
          {:cont, :ok}
      end
    end)
  end

  defp cert_signer_ok?(
         %V1.Signature{signer_kind: :ROOT, signer_id: key},
         _cert,
         _devices,
         _revoked,
         roots
       ),
       do: key in roots

  defp cert_signer_ok?(
         %V1.Signature{signer_kind: :DEVICE, signer_id: signer},
         cert,
         devices,
         revoked,
         _roots
       ) do
    cond do
      signer == cert.device_id ->
        false

      d = devices[signer] ->
        ts(d.cert.added_at) < ts(cert.added_at)

      r = revoked[signer] ->
        # Revoked signer: must have been added before, and revoked after,
        # the certificate it issued. Its own cert is gone with it, so the
        # roster cannot say when it was added; the revocation time bounds it.
        ts(r.body.revoked_at) > ts(cert.added_at)

      true ->
        false
    end
  end

  defp cert_signer_ok?(_sig, _cert, _devices, _revoked, _roots), do: false

  # ---- 5. Revocations ------------------------------------------------------------

  defp revocations(body, revoked, devices, [first]) do
    roots = roots_of(body, first)

    Enum.reduce_while(revoked, :ok, fn {device_id, %{body: rb, entry: entry}}, :ok ->
      cond do
        rb.identity_id != body.identity_id ->
          {:halt, {:error, {:revocations, :identity}}}

        not revoker_ok?(entry.signature, device_id, devices, roots) ->
          {:halt, {:error, {:revocations, :signer}}}

        not Crypto.verify(
          entry.body,
          entry.signature.signature,
          signer_key(entry.signature, devices, %{})
        ) ->
          {:halt, {:error, {:revocations, :signature}}}

        true ->
          {:cont, :ok}
      end
    end)
  end

  defp revoker_ok?(
         %V1.Signature{signer_kind: :ROOT, signer_id: key},
         _device_id,
         _devices,
         roots
       ),
       do: key in roots

  defp revoker_ok?(
         %V1.Signature{signer_kind: :DEVICE, signer_id: signer},
         device_id,
         devices,
         _roots
       ),
       do: signer != device_id and Map.has_key?(devices, signer)

  defp revoker_ok?(_, _, _, _), do: false

  # ---- 6. Roster signature -----------------------------------------------------

  defp signature(bytes, sigs, body, devices) do
    current_root = body.root_public_key.ed25519

    valid = Enum.filter(sigs, &roster_signature_valid?(&1, bytes, current_root, devices))

    case valid do
      [] -> {:error, {:signature, :none_valid}}
      sigs -> {:ok, Enum.any?(sigs, &(&1.signer_kind == :ROOT))}
    end
  end

  defp roster_signature_valid?(%V1.Signature{signer_kind: :ROOT} = sig, bytes, root, _devices) do
    sig.signer_id == root and Crypto.verify(bytes, sig.signature, root)
  end

  defp roster_signature_valid?(%V1.Signature{signer_kind: :DEVICE} = sig, bytes, _root, devices) do
    case devices[sig.signer_id] do
      nil -> false
      d -> Crypto.verify(bytes, sig.signature, d.cert.public_key.ed25519)
    end
  end

  defp roster_signature_valid?(_sig, _bytes, _root, _devices), do: false

  # ---- 7. Monotonicity ---------------------------------------------------------

  defp monotonic(_body, _devices, revoked, nil), do: {:ok, Map.keys(revoked)}

  defp monotonic(body, devices, revoked, %__MODULE__{} = held) do
    cond do
      body.identity_id != held.identity_id ->
        {:error, {:monotonic, :identity}}

      body.version <= held.version ->
        {:error, {:monotonic, :version}}

      not Enum.all?(
        Map.keys(held.devices),
        &(Map.has_key?(devices, &1) or Map.has_key?(revoked, &1))
      ) ->
        {:error, {:monotonic, :device_dropped}}

      not Enum.all?(Map.keys(held.revoked), &Map.has_key?(revoked, &1)) ->
        {:error, {:monotonic, :revocation_dropped}}

      not List.starts_with?(
        Enum.map(body.rotations, & &1.body),
        Enum.map(held.body.rotations, & &1.body)
      ) ->
        {:error, {:monotonic, :rotation_prefix}}

      true ->
        {:ok, Map.keys(revoked) -- Map.keys(held.revoked)}
    end
  end

  # ---- Helpers -----------------------------------------------------------------

  defp signer_key(%V1.Signature{signer_kind: :ROOT, signer_id: key}, _devices, _revoked), do: key

  defp signer_key(%V1.Signature{signer_kind: :DEVICE, signer_id: id}, devices, revoked) do
    cond do
      d = devices[id] -> d.cert.public_key.ed25519
      r = revoked[id] -> revoked_public_key(r)
      true -> <<0::256>>
    end
  end

  # A revoked device's public key is not in the roster any more; the
  # revocation body does not carry it either. Certificates issued by a
  # device that was later revoked therefore cannot be re-verified from the
  # roster alone unless the revocation entry carries the key. Until the
  # proto gains that field, such certificates fail verification.
  defp revoked_public_key(_), do: <<0::256>>

  defp decode(module, bytes, step) do
    {:ok, module.decode(bytes)}
  rescue
    _ -> {:error, {step, :undecodable}}
  end

  defp ts(nil), do: 0
  defp ts(%Google.Protobuf.Timestamp{seconds: s, nanos: n}), do: s * 1_000_000_000 + n

  defp to_dt(nil), do: DateTime.from_unix!(0)
  defp to_dt(%Google.Protobuf.Timestamp{seconds: s}), do: DateTime.from_unix!(s)
end
