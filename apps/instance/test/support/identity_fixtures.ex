defmodule Lumia.IdentityFixtures do
  @moduledoc """
  Builds real rosters with real Ed25519 keys for tests: the client side of
  docs/specs/identity.md, in miniature.
  """

  alias Lumia.Crypto
  alias Lumia.Proto
  alias Lumia.V1

  def now(offset_seconds \\ 0),
    do: Proto.timestamp(DateTime.add(DateTime.utc_now(), offset_seconds, :second))

  @doc "A fresh identity: root keypair and identity id."
  def identity do
    {pub, priv} = Crypto.generate_ed25519()
    %{root_pub: pub, root_priv: priv, identity_id: Crypto.blake3(pub)}
  end

  @doc "A fresh device keypair with an id."
  def device(label \\ "device") do
    {pub, priv} = Crypto.generate_ed25519()
    %{id: Proto.uuid(Ecto.UUID.generate(version: 7)), pub: pub, priv: priv, label: label}
  end

  @doc "A device certificate signed by the root (`added_by` empty) or by `signer` (a device)."
  def cert(identity, device, signer \\ :root, opts \\ []) do
    body = %V1.DeviceCertBody{
      identity_id: identity.identity_id,
      device_id: device.id,
      public_key: %V1.PublicKey{ed25519: device.pub},
      kind: Keyword.get(opts, :kind, :NATIVE),
      added_at: Keyword.get(opts, :added_at, now()),
      added_by: if(signer == :root, do: "", else: signer.id),
      web_origin: Keyword.get(opts, :web_origin, "")
    }

    bytes = V1.DeviceCertBody.encode(body)

    signature =
      case signer do
        :root ->
          %V1.Signature{
            signer_kind: :ROOT,
            signer_id: identity.root_pub,
            signature: Crypto.sign(bytes, identity.root_priv)
          }

        s ->
          %V1.Signature{
            signer_kind: :DEVICE,
            signer_id: s.id,
            signature: Crypto.sign(bytes, s.priv)
          }
      end

    %V1.DeviceEntry{
      cert_body: bytes,
      authorization: signature,
      device_id: body.device_id,
      public_key: body.public_key,
      kind: body.kind,
      added_at: body.added_at,
      added_by: body.added_by,
      web_origin: body.web_origin,
      label: device.label
    }
  end

  @doc "A revocation of `device` signed by the root or by `signer`."
  def revocation(identity, device, signer \\ :root, opts \\ []) do
    body = %V1.RevocationBody{
      identity_id: identity.identity_id,
      device_id: device.id,
      revoked_at: Keyword.get(opts, :revoked_at, now()),
      revoked_by: if(signer == :root, do: "", else: signer.id),
      public_key: %V1.PublicKey{ed25519: device.pub},
      added_at: Keyword.get(opts, :added_at, now(-60))
    }

    bytes = V1.RevocationBody.encode(body)

    signature =
      case signer do
        :root ->
          %V1.Signature{
            signer_kind: :ROOT,
            signer_id: identity.root_pub,
            signature: Crypto.sign(bytes, identity.root_priv)
          }

        s ->
          %V1.Signature{
            signer_kind: :DEVICE,
            signer_id: s.id,
            signature: Crypto.sign(bytes, s.priv)
          }
      end

    %V1.RevokedEntry{
      body: bytes,
      signature: signature,
      device_id: body.device_id,
      revoked_at: body.revoked_at,
      revoked_by: body.revoked_by
    }
  end

  @doc "A rotation from `identity`'s root to `new_identity`'s root, signed by the old root."
  def rotation(identity, new_root_pub) do
    body = %V1.RootRotationBody{
      identity_id: identity.identity_id,
      from: %V1.PublicKey{ed25519: identity.root_pub},
      to: %V1.PublicKey{ed25519: new_root_pub},
      rotated_at: now()
    }

    bytes = V1.RootRotationBody.encode(body)

    %V1.RootRotation{
      body: bytes,
      signature: %V1.Signature{
        signer_kind: :ROOT,
        signer_id: identity.root_pub,
        signature: Crypto.sign(bytes, identity.root_priv)
      }
    }
  end

  @doc """
  Builds and signs a roster. `signer` is `:root` or a device. Options:
  `version`, `devices`, `revoked`, `rotations`, `root_pub` (current root).
  """
  def roster(identity, signer, opts \\ []) do
    body = %V1.RosterBody{
      identity_id: identity.identity_id,
      version: Keyword.get(opts, :version, 1),
      root_public_key: %V1.PublicKey{ed25519: Keyword.get(opts, :root_pub, identity.root_pub)},
      rotations: Keyword.get(opts, :rotations, []),
      devices: Keyword.get(opts, :devices, []),
      revoked: Keyword.get(opts, :revoked, []),
      issued_at: now()
    }

    bytes = V1.RosterBody.encode(body)

    signature =
      case signer do
        :root ->
          %V1.Signature{
            signer_kind: :ROOT,
            signer_id: body.root_public_key.ed25519,
            signature: Crypto.sign(bytes, Keyword.get(opts, :root_priv, identity.root_priv))
          }

        s ->
          %V1.Signature{
            signer_kind: :DEVICE,
            signer_id: s.id,
            signature: Crypto.sign(bytes, s.priv)
          }
      end

    %V1.SignedRoster{body: bytes, signatures: [signature]}
  end

  @doc "The common starting point: one identity, one root-signed device, roster v1 signed by the root."
  def bootstrap do
    identity = identity()
    device = device("phone")
    roster = roster(identity, :root, devices: [cert(identity, device)])
    %{identity: identity, device: device, roster: roster}
  end
end
