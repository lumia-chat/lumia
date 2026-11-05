# Identity, Devices, and Recovery

Status: accepted, 2026-11-01. This specification makes section 5 of [architecture.md](../architecture.md) implementable. Wire shapes are in `proto/lumia/v1/identity.proto` and `api.proto`; endpoint mappings are in [protocol.md](../protocol.md). Where this document and the architecture disagree, the architecture wins and this document is wrong.

## 1. Model

```text
Recovery phrase    24 BIP39 words; the only secret a person has to keep
Root key           Ed25519, derived from the phrase; offline by default
Backup key         256-bit, derived from the phrase; on every native device
Identity ID        BLAKE3-256 of the first root public key; never changes
Roster             signed, versioned document: devices, revocations, rotations
Device key         Ed25519 per device; signs everything day to day
Instance account   authorization on one instance, bound to an identity ID
Session            per device per instance, bound to a device key
```

The root key authorizes. Device keys act. The roster is how instances learn which device keys may act for an identity, and it is **self-certifying**: every entry in it carries a signature that chains to a root, so an instance that has never seen the identity can verify any roster version from the bytes alone.

Instances never talk to each other (architecture hard rule 1). The client carries the roster to every instance and the design below is what makes that safe.

## 2. Keys and derivation

**Phrase.** 24 words from the BIP39 English wordlist, encoding 256 bits of entropy plus the BIP39 8-bit checksum. The checksum catches typos; BIP39's PBKDF2 seed step is not used.

**Derivation.** With `entropy` as the 32 bytes the phrase encodes:

```text
root_seed  = HKDF-SHA256(ikm = entropy, salt = "", info = "lumia/root/v1",   L = 32)
backup_key = HKDF-SHA256(ikm = entropy, salt = "", info = "lumia/backup/v1", L = 32)
root       = Ed25519 keypair from root_seed
```

The phrase alone recovers the root and the backup key. No server holds anything the root depends on.

**Identity ID.** `identity_id = BLAKE3-256(first_root_public_key)`, 32 bytes. Rendered as lowercase hex in JSON. It is the stable name of the identity across root rotations.

**Device keys.** Ed25519, generated on the device, stored in the platform keystore (Secure Enclave, Android Keystore, DPAPI, Secret Service) where available. `device_id` is a UUIDv7 minted by the device.

**Backup key.** XChaCha20-Poly1305 key for the identity backup (section 8) and the history backup (architecture 11.3). Held by every native device; never by a web device (section 7).

**Safety number.** For identities A and B with current root keys:

```text
inputs = sort_lexicographic([identity_id_A || root_pk_A, identity_id_B || root_pk_B])
digest = BLAKE3-256(inputs[0] || inputs[1])
```

Rendered as 60 decimal digits in groups of five, taken from the digest interpreted as a big-endian integer modulo 10^60. It changes when either side rotates its root; the client says so when it happens.

## 3. The roster

### 3.1 Structure

A `SignedRoster` is a serialized `RosterBody` plus one or more signatures over those exact bytes. The body holds:

| Field | Meaning |
|---|---|
| `identity_id` | Must equal BLAKE3-256 of the first root public key. |
| `version` | Monotonic. Starts at 1. |
| `root_public_key` | The current root. |
| `rotations` | Oldest first. Each is a `RootRotationBody` (`from`, `to`, `rotated_at`) signed by `from`. The chain starts at the key whose hash is `identity_id` and ends at `root_public_key`. |
| `devices` | One `DeviceEntry` per authorized device, each carrying a `DeviceCertBody` signed by its authorizer. |
| `revoked` | One `RevokedEntry` per revoked device, each carrying a `RevocationBody` signed by the revoker. The body also carries the revoked device's public key and its certificate's `added_at`, because its `DeviceEntry` leaves the roster and certificates it issued before revocation must stay verifiable. A device appears in exactly one of `devices` or `revoked`. |
| `issued_at` | Self-asserted; informational only. |

Mirror fields on `DeviceEntry` and `RevokedEntry` (`device_id`, `public_key`, timestamps) exist for convenience. Verifiers read the signed bodies and reject any entry whose mirrors disagree with its body.

### 3.2 Who may sign what

| Entry | Valid signers |
|---|---|
| Device certificate | The root; or a device that was authorized before `added_at` and not revoked before `added_at`. |
| Revocation | The root; or a device that is authorized and not revoked in the same roster body, other than the device being revoked. |
| Root rotation | The predecessor root (`from`). |
| Roster (the document) | The current root; or a device that is authorized and not revoked in this body. |

Revoking a device never invalidates certificates it issued earlier, because authorization is judged at `added_at`, not at verification time. A device that was itself added by a later-revoked device stays valid.

### 3.3 Verification order

An instance runs these steps, in order, on every roster it receives, whether by publish, in an identity proof, or in a revoke request. Any failure rejects the roster with `IDENTITY_PROOF_INVALID` and `details.step` naming the step.

1. **Parse.** Deserialize `body`. Reject if any `DeviceEntry` or `RevokedEntry` mirrors disagree with its signed body, or if a device appears in both lists, or if the roster exceeds the limits in section 11.
2. **Identity.** Compute the first root: `rotations[0].from` if rotations exist, else `root_public_key`. Reject unless `identity_id == BLAKE3-256(first_root)`.
3. **Rotation chain.** For each rotation in order: `from` must equal the previous rotation's `to` (or the first root), the signature must verify under `from`, and the last `to` must equal `root_public_key`.
4. **Device certificates.** For each device: verify `authorization` over `cert_body`. If the signer is ROOT, the signer key must be a root in the chain. If the signer is DEVICE, the signer must be a device in `devices` or `revoked` with `added_at` earlier than this entry's `added_at`, and if revoked, `revoked_at` later than this entry's `added_at`; a revoked signer's key and `added_at` come from its `RevocationBody`. Resolve in `added_at` order so chains terminate. Reject any certificate whose `added_at` is more than five minutes ahead of the instance's clock when first seen, because `added_at` decides authorization and a device could otherwise pre-date certificates past its own future revocation.
5. **Revocations.** For each revoked entry: verify `signature` over `body`. Signer is a root in the chain, or a device in `devices` other than the revoked one.
6. **Roster signature.** At least one signature in `signatures` verifies over `body` and is by the current root or by a device in `devices`.
7. **Monotonicity.** If the instance holds a version for this identity: reject unless `version` is greater, every device in the held version is in `devices` or `revoked`, every revocation in the held version is in `revoked`, and the held rotation chain is a prefix of this one. Equal version with identical bytes is a no-op; equal version with different bytes is a conflict (section 5).
8. **Revocation policy.** If `revoked` contains any device not revoked in the held version, accept only when the roster arrived through the revoke endpoint with a valid reauth token for this account, or when `signatures` includes a valid ROOT signature. Otherwise reject with `POLICY_DENIED` and `details.reason = "reauth_required"`.

On acceptance the instance stores the roster as the held version, terminates sessions and gateway connections of any newly revoked device (protocol 6.8, `DISCONNECT_DEVICE_REVOKED`), and emits `RosterUpdated` on the account log.

### 3.4 First contact

An instance with no held version accepts any roster that passes steps 1 to 6. There is nothing to compare against; the certificate chain is the whole guarantee. A directory (architecture 5.2) additionally pins the first root key it sees for a handle and treats a different first root as a different identity, never as a rotation.

## 4. Flows

### 4.1 Create an identity

On a device with no identity:

1. Generate 256 bits of entropy; derive root and backup key (section 2); show the phrase and require the user to confirm it.
2. Generate the device key.
3. Build roster version 1: the device, with a certificate signed by the root; sign the roster with the root.
4. Discard the root private key from memory. Keep the backup key in the keystore.
5. Create the flagship account by identity proof unless the user declines (architecture 5.2); write the identity backup (section 8).

### 4.2 Add a device, with an existing device

Actors: **N**, the new device; **E**, an existing authorized native device.

1. N generates its device key and an ephemeral X25519 key, and shows a QR encoding a `LinkRequest` (`device_id`, device public key, ephemeral public key, kind, label, web origin for WEB, 32-byte nonce).
2. E scans it and shows what it is about to authorize: kind, label, and for WEB the origin. The user approves with E's own unlock (biometric or passcode).
3. E builds N's certificate (`DeviceCertBody` with `added_by = E`), signed by E's device key; builds roster version held+1 with N appended; signs it with E's device key.
4. E publishes the roster to every instance it has a session on, in parallel. Each either accepts or returns `CONFLICT` (section 5). E does not continue until the instance it will use for the bundle has accepted.
5. E generates its own ephemeral X25519 key, derives `shared = HKDF-SHA256(X25519(E_eph_priv, N_eph_pub), info = "lumia/link/v1")`, and encrypts a `LinkBundleBody` (roster, backup key unless N is WEB, host accounts, pinned keys, the nonce) with XChaCha20-Poly1305 under `shared`.
6. E uploads it with `POST /identity/link-bundles` to the flagship if it has a session there, else any instance it has a session on. The response is an 8-character code. E shows the code and the host.
7. The user types the code into N. N calls `GET /identity/link-bundles/{code}?device_id=N` on that host, derives `shared` from the returned sender ephemeral key, decrypts, checks the nonce matches, and verifies the roster (section 3.3 steps 1 to 6) and that it contains N.
8. N signs in by identity proof at each host in the bundle where policy allows, and by credentials elsewhere. N stores the backup key in its keystore. N is done.

The bundle is single-use and expires ten minutes after creation. A fetch with the wrong `device_id` is `NOT_FOUND`. The root key is never in the bundle and never on E.

### 4.3 Add a device, with no existing device

1. On the new device, enter the phrase; derive root and backup key.
2. Fetch the identity backup from any host the user names (section 8), decrypt, and take the roster from it. If no backup is reachable, start from the newest roster any instance returns for the identity, or from nothing.
3. Build the next version: revoke every device currently in `devices` (each revocation signed by the root), add this device with a root-signed certificate, sign the roster with the root. Revoking all old devices is the default; the user may keep specific ones.
4. Discard the root private key. Publish the roster everywhere by identity proof sign-in; root-signed rosters pass the revocation policy without reauth.
5. Rejoin MLS groups per architecture 11.3: by external commit, since no device remains to add this one.

### 4.4 Revoke a device

From any authorized device D, revoking device X:

1. D builds a `RevocationBody` for X signed by D, and roster version held+1 with X moved to `revoked`; signs the roster with D.
2. For each instance the client knows, in the order the user chooses (the UI lists them with state): D calls `POST /auth/reauth` with a credential, then `POST /identity/devices/revoke` with the roster and the reauth token.
3. The instance verifies (section 3.3, including step 8), stores, kills X's sessions and connections there, and emits `RosterUpdated`.
4. The UI shows "revoked on k of n" with the unreachable or unauthenticated instances named. It retries them on the next connect and keeps the state visible until all confirm. "Globally revoked" is never shown.

X, if online, receives `DISCONNECT_DEVICE_REVOKED` and wipes its local store (architecture 12). A thief holding X has X's device key but not the account credentials, so X cannot produce an acceptable revocation of D at any instance; the root path (4.3) always beats X because root signatures bypass reauth.

### 4.5 Rotate the root

Requires the old phrase. On a device:

1. Enter the old phrase; derive the old root. Generate a new phrase; derive the new root and new backup key.
2. Build a `RootRotationBody` (`from` old, `to` new) signed by the old root; append it to `rotations`; set `root_public_key` to the new root.
3. Re-sign nothing else: existing certificates stay valid because the old root remains in the chain.
4. Sign the roster with the new root; discard both private roots. Publish everywhere.
5. Re-encrypt the identity backup and history backup manifests under the new backup key (architecture 11.3). Distribute the new backup key to native devices through the next link or through an encrypted account-log message to each device.

A lost phrase cannot be rotated. The identity remains usable through its devices; recovery is impossible; the client says this at setup.

### 4.6 Link an existing account to an identity

An account created with a password before the user had an identity, or on an instance that requires OIDC:

1. The client authenticates to the account normally and calls `POST /auth/reauth`.
2. It requests a `LINK_ACCOUNT` challenge, signs it with the device key, and calls `POST /identity/link` with the proof and the reauth token.
3. The instance verifies both and binds the account to the identity. Identity-proof sign-in to that account thereafter follows the instance's policy (architecture 6.3).

Matching an email or profile is never sufficient for linking.

## 5. Conflicts and merging

**Same version, different bytes.** A ROOT-signed roster beats a DEVICE-signed one: the instance replaces what it holds without the monotonicity step, since the root is the identity's own authority and this is how a recovery from the phrase displaces a thief's fork; the revocations it adds are applied as on any accepted roster. Two DEVICE-signed forks, or a DEVICE-signed roster against a ROOT-signed one: the instance keeps the one it holds and answers `CONFLICT` with the held roster in `details.held` (base64). The client must not retry the rejected bytes.

**Client merge.** On `CONFLICT`, or when two instances return different held rosters at the same version, the client builds version `max(versions) + 1`:

- `devices` = union of both forks' devices, minus any device revoked in either fork;
- `revoked` = union of both forks' revocations;
- `rotations` = the longer chain, which must have the shorter as a prefix, else the forks are irreconcilable and the client refuses and alerts the user.

Each entry keeps its original certificate or revocation signature, so the merged body still verifies. The client signs the merged roster and publishes it everywhere: it is stored on the device at once and reaches each instance with the device's next proof there, where it is accepted as a successor of whichever fork that instance held. Revocations always win a merge; a merged roster that adds revocations relative to what some instance holds goes through the revoke endpoint there.

One case needs more than the union. A device certified by a signer at or after that signer's revocation in the other fork cannot verify (the certificate rule of section 3.3, step 4), and monotonicity forbids dropping a device the instance holds, so the merge revokes it, with a revocation signed by the merging device: a revoked device's later additions do not count. A fork that revokes the merging device itself is not merged; the device reports itself revoked.

**Older roster.** An instance that receives a version lower than it holds ignores it and returns the held version. The client adopts it and reconciles as above.

## 6. Link bundles and codes

- Code: 8 characters from the Crockford base32 alphabet, 40 bits, generated by the instance; collision is retried.
- Stored with: ciphertext, sender ephemeral public key, nonce, target `device_id`, `created_at`. Single use: the first successful fetch deletes it. Expires ten minutes after creation; Oban purges.
- `GET` is public and rate-limited per source address and per code (five attempts, then the bundle is deleted).
- Ciphertext limit 64 KB.
- The bundle host is any instance the existing device has a session on. The new device learns the host from the user, alongside the code.

## 7. Web devices

A WEB device key is scoped to the origin that served the client (architecture 11.4). Its certificate carries `web_origin`. Linking a web device follows 4.2 with the browser as N and a native device as E; the bundle omits the backup key. A web device therefore has live MLS membership, server content, and whatever personal DM traffic it received while open, and cannot restore history. The device list labels web devices with their origin. An instance refuses identity proofs from a WEB device whose `web_origin` is not the instance's own origin or the flagship's.

## 8. Identity backup

**Contents** (`IdentityBackupBody`): version, roster, signed profile, host accounts, pinned contact keys with verification state, block list, `written_at`.

**Encryption.** XChaCha20-Poly1305 under the backup key with a fresh 24-byte nonce per write.

**Storage.** `PUT /identity/backup` to every instance the identity has an account on, after any change to the contents. Versioned; an instance keeps the highest version it receives. `GET /identity/backup` from any one of them on recovery. The ciphertext is small (kilobytes) and hosts cannot read it.

**On recovery** (4.3), the backup supplies the roster, the hosts to sign in to, the pinned keys so safety numbers survive, and the block list so blocks survive.

## 9. Challenge and proof

As in protocol section 13, with these details fixed:

- `IdentityChallengeBody.mac` is HMAC-SHA256 under an instance secret over `nonce || host || expires_at || purpose`. Issuing a challenge stores nothing.
- Expiry is two minutes.
- Using a proof inserts `nonce` into `used_challenges (nonce primary key, expires_at)`. A duplicate insert means replay and fails with `IDENTITY_PROOF_INVALID`, across all nodes, because the insert is in Postgres. Oban purges expired rows.
- The client refuses to sign a challenge whose `host` is not the host it is talking to.
- The proof's roster is verified per section 3.3 before the device signature over the challenge is checked, so a proof from a revoked device fails on the roster, not on the signature.

## 10. Reauth

`POST /auth/reauth` accepts a password, a TOTP code, or a WebAuthn assertion for the current account and returns a token valid for five minutes, bound to the session, usable only by the revoke and link endpoints. Each use is logged on the account log. Accounts without a credential (joined by identity proof only, bots) cannot reauth and therefore cannot revoke through the device path; the root path (4.3) remains.

## 11. Limits

| Limit | Value |
|---|---|
| Devices per identity | 20 |
| Revocations retained in a roster | 100; the client may drop the oldest once every host it knows holds a version containing them |
| Rotations retained | unbounded, but at most one per day is accepted |
| Serialized roster | 64 KB |
| Roster publishes per identity | 30 per hour per instance |
| Link bundle ciphertext | 64 KB |
| Link code attempts | 5 per code |

## 12. Tests

Failure-injection tests that must pass before this area is considered implemented:

```text
Revoke device X from D while instance C is unreachable; reconnect to C; verify C
  accepts only through revoke-with-reauth and the UI showed "2 of 3" meanwhile.
Thief device X publishes a roster revoking D without reauth at every instance;
  verify every instance rejects with reauth_required and D's sessions survive.
Devices D1 and D2 each add a device concurrently at different hosts, producing
  two forks at the same version; verify the client merges, both new devices
  survive, and every instance converges on the merged version.
Publish a roster that omits a revocation the instance holds; verify rejection
  at step 7.
Present a device certificate signed by a device that was revoked before the
  certificate's added_at; verify rejection at step 4.
Present a device certificate signed by a device revoked after the certificate's
  added_at; verify acceptance.
Present a device certificate whose added_at is in the future relative to the
  verifying instance's clock by more than the allowed skew; verify rejection at
  step 4, so a device about to be revoked cannot pre-date certificates past its
  own revocation.
Replay an identity proof; verify the second use fails on the used_challenges
  insert on a different node than the first.
Fetch a link bundle twice; verify the second fetch is NOT_FOUND.
Fetch a link bundle with the wrong device_id; verify NOT_FOUND and that five
  wrong attempts delete the bundle.
Recover from the phrase with every device gone; verify the root-signed roster
  passes the revocation policy without reauth and MLS groups are rejoined by
  external commit.
Rotate the root; verify old certificates still verify, safety numbers change,
  and an instance holding the pre-rotation roster accepts the new one at step 3.
Present a WEB device proof from an origin other than the instance's or the
  flagship's; verify rejection.
```
