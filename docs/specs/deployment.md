# Deployment

Status: accepted, 2026-11-02. This specification retires section 15 of [architecture.md](../architecture.md): what an instance is made of, how it is backed up, restored, clustered, upgraded and observed. It is written for the operator and for the code that serves the operator; the install guide quotes it.

## 1. What an instance is

One instance is **one database, one blob store, one master key**, served by one or more identical nodes of the Lumia image. The database holds everything the protocol defines; the blob store holds attachment ciphertext (architecture 12); the master key wraps the blobs' content keys. The three are one unit: a database without its master key cannot open a single attachment, and blobs without the database are unreachable ciphertext.

The web UI (`apps/web`) ships inside the image and is served by the instance from `priv/static/ui`, so a self-hosted instance's web origin is itself (architecture 11.4).

## 2. Profiles

### 2.1 Single node

Two containers: the Lumia image and Postgres, plus three persistent volumes: the database, the blobs, and the secrets (the master key and `SECRET_KEY_BASE`). This is the default and what `apps/instance/deploy/compose.yml` describes. Off-host backups of the database volume and the blob volume complete it (section 4).

### 2.2 Multi-node

The same image on N nodes, all pointing at one Postgres and one **shared** blob store, clustered by DNS. Preconditions, enforced at boot:

- `DNS_CLUSTER_QUERY` names a DNS record that resolves to every node (a Kubernetes headless service, a Fly app, a scaled Compose service). Each node polls it and connects to the others; no other discovery exists (decided in the step 7 grill: one DNS name with one record per node is what every supported deployment provides, and the nodes need nothing more).
- The blob store is S3-compatible or a shared filesystem. **Local-disk blobs are refused when clustering is on**, because a blob written on one node would be missing on the next. Until attachments exist (architecture 12 is not built), this is a configuration rule with nothing to refuse; it is stated here so the code that adds blobs inherits it.
- Postgres availability (replicas, failover) and the per-node pool size are the operator's; the install guide recommends `DB_POOL_SIZE` of 10 per node per 4 vCPU and `max_connections` sized for N nodes plus the backup command.

Nodes share nothing but the database and the blob store. Live delivery crosses nodes through the cluster's pub/sub; what pub/sub misses, each node's reconciler recovers from the database within one tick ([delivery.md](delivery.md) section 7). Section 8 lists the cross-node cases the tests must pass.

### 2.3 Two instances, for development

`apps/instance/deploy/compose.two.yml` runs two single-node instances side by side from the same image, each with a Postgres of its own: the flagship, the one a new identity gets an account on by default (architecture 6.1; its directory is its account table), at `http://localhost:4100`, and a community instance at `http://127.0.0.1:4200`, over plain http and without the proxy; the two names are deliberate, since an identity proof is bound to the host name a client reaches an instance by, and each instance's `PHX_HOST` is that name. They share nothing but the people who hold accounts on both. `deploy/two-instances.sh` builds the image, brings them up, and runs the cross-instance cases with the CLI: one identity with an account on each by proof, its profile travelling with the proof, a server and an invite on the community, a second identity joining through it and posting, and the scenario suite against each. `--keep` leaves them up for the clients (`http://localhost:4100` and `:4200` as instances).

## 3. Configuration

All configuration is environment variables read at boot (`config/runtime.exs`); none is read later. The instance refuses to start when a required one is missing or malformed, naming it.

| Variable | Required | Meaning |
|---|---|---|
| `DB_URL` | yes | Postgres, with the credentials |
| `DB_SSL` | no | `true` connects over TLS verified against the system trust store, for a database with a publicly signed certificate (Fly Managed Postgres); default `false`. An `sslmode` in the URL is ignored |
| `DB_CA_CERT` | managed Postgres | The PEM of the CA that signed the database's certificate, for one the system store does not trust (DigitalOcean, RDS); TLS on, verified against that CA with the host-name check. Takes precedence over `DB_SSL` |
| `SECRET_KEY_BASE` | yes | Signs sessions and challenges; at least 64 bytes |
| `PHX_HOST` | yes | The public host name; the instance's own web origin is `https://` plus this |
| `PORT` | no | HTTP listener, default 4000; TLS terminates at the operator's proxy |
| `LUMIA_MASTER_KEY_FILE` | with blobs | Path to the 32-byte instance master key (section 5) |
| `LUMIA_BLOB_STORE` | with blobs | `local:<path>`, `s3://bucket` with the usual AWS variables, or `fs:<shared path>` |
| `DNS_CLUSTER_QUERY` | multi-node | The DNS name that resolves to every node |
| `RELEASE_COOKIE` | multi-node | The Erlang distribution cookie, the same on every node. Each image build generates its own, so without this a rolling upgrade's new nodes cannot join the old ones (section 6) |
| `DB_POOL_SIZE` | no | Database connections per node, default 10 |
| `LUMIA_WEB_ORIGINS` | no | Comma-separated flagship origins allowed to present web devices and call the API (identity §7); the instance's own origin is always allowed |
| `LUMIA_OPERATOR_NAME`, `LUMIA_OPERATOR_URL`, `LUMIA_TERMS_URL`, `LUMIA_PRIVACY_URL` | no | The trust card (`GET /instance`) |
| `LUMIA_OPEN_REGISTRATION` | no | `true` (default) or `false` |
| `LUMIA_METRICS` | no | `true` exposes `/metrics` (section 7); default `false` |
| `LUMIA_PUSH_RELAY_URL` | for APNs and FCM | The push relay (architecture 15.5); without it, relay subscriptions are kept and skipped |
| `LUMIA_PUSH_CONTACT` | no | The address the Web Push VAPID token names (`mailto:`); default `ops@` the host |

## 4. Backups and restore

### 4.1 The backup set

A backup is **the database, the blob store, and the master key, taken together**. The release ships two commands that produce and consume exactly that set:

```
bin/lumia backup <dir>
bin/lumia restore <dir>
```

`backup` writes into `<dir>`:

- `database.dump`: `pg_dump` in custom format, taken with `--serializable-deferrable` so it is one consistent snapshot while the instance keeps running;
- `blobs/`: the blob store's contents, for `local:` and `fs:` stores (an `s3://` store is backed up by the bucket's own versioning or replication, and the manifest says so);
- `manifest.json`: the schema version (the last migration), the recovery generation, the blob store kind, the instance host, the time, and the **fingerprint of the master key** (BLAKE3 of the key), **never the key itself**. The key is the operator's to keep separately; a backup directory on its own cannot open an attachment.

The image includes the Postgres client tools of the Postgres major it supports, so `backup` works inside the container.

### 4.2 Restore

`restore <dir>` on an instance whose database is empty or whose operator passed `--replace`:

1. reads the manifest and refuses when the master key in `LUMIA_MASTER_KEY_FILE` does not match its fingerprint (with blobs), or when the dump's schema version is newer than the running release's migrations;
2. `pg_restore`s the dump and runs any newer migrations;
3. copies the blobs back for `local:` and `fs:` stores and **verifies every blob reference in the database resolves** to a stored object, listing the missing ones and failing unless `--allow-missing-blobs`;
4. **bumps the recovery generation** (architecture 8.4): every client that connects afterwards sees a generation it does not know, discards its cursors and takes the snapshot path, which is what makes a restore behind a client's cursor safe ([delivery.md](delivery.md) section 12, "restore behind a cursor").

Restoring onto a running cluster is not supported: stop every node, restore on one, start them all.

### 4.3 What the operator is told

Recovery objectives are the operator's to choose. The install guide says plainly: a database backup without the master key loses every attachment; a restore makes every client resynchronise once; and the time between backups is the most content a restore can lose.

## 5. Encryption at rest, by responsibility

| Data | Who encrypts | Mechanism |
|---|---|---|
| Blobs | Lumia | Envelope encryption under the instance master key (architecture 12) |
| Personal DM content and history chunks | The clients | MLS and the backup key; the server never has plaintext ([e2ee.md](e2ee.md)) |
| Postgres data files, WAL, database backups | **The operator** | Filesystem or block-device encryption, encrypted backup storage. Lumia does not provide this and the install guide says so |
| Instance master key | The operator | A file the operator mounts (`LUMIA_MASTER_KEY_FILE`); a KMS integration later |
| Client local stores | Lumia | A key in the platform keystore natively; the wrapped store key on the web ([e2ee.md](e2ee.md) section 8) |

## 6. Upgrades

Migrations run on boot (`bin/lumia migrate`, which the entrypoint runs before the server) behind a Postgres advisory lock, so two nodes starting at once run them once. The lock prevents two runners, not incompatibility; that comes from the rule:

**Expand and contract.** A release may only add tables, columns and indexes that the previous release tolerates. A destructive change (dropping, renaming, tightening a constraint the old code violates) ships one release after the code stopped depending on the old shape. Nodes of release N and N-1 may coexist during a rolling upgrade; N-2 is unsupported, and the migration runner refuses to run when the database's recorded release is older than N-1.

Rolling upgrade: with DNS clustering, replace nodes one at a time; sessions on a stopped node reconnect to another and resume from their cursors ([delivery.md](delivery.md) section 6).

## 7. Observability

`/metrics` serves Prometheus text when `LUMIA_METRICS` is `true`; it is meant for the operator's network, not the public one, and the guide shows how to keep it off the proxy. The metrics are the ones `LumiaWeb.Telemetry` defines (request and query durations, VM memory and run queues) plus the instance's own: gateway connections, sessions, commands acknowledged, events applied, reconciler ticks and the heads they found behind.

OpenTelemetry traces and opt-in update checks and anonymous statistics are later; hard rule 3 of the architecture applies to all of them.

Logs go to stdout, one line per request and per gateway connection, never with a token, a password, or message content; a JSON formatter for the release is later.

## 8. Tests

The cross-node cases of [delivery.md](delivery.md) section 12, run on two BEAM nodes of the test suite (the second started by OTP's `:peer`, both on one database, a partition made by `Node.disconnect`):

```text
Commit an event on node B without broadcasting; verify a session on node A
  receives it within one reconciler tick.
Partition B from A, commit and broadcast on B, heal; verify A's session
  receives the event through reconciliation and nothing twice.
Remove a member on node A while node B holds the old resolution; verify B
  resolves no membership afterwards and the member's account log carries
  the removal (the member-level path of the authorization spec), so nothing
  committed after the removal is delivered.
Deliver broadcasts from B for seq N+2 then N+1 to a session on A holding N;
  verify the client receives N+1 then N+2 and nothing twice.
```

And for this document's own commands:

```text
backup then restore into an empty database; verify every table's row count
  matches, the recovery generation went up by one, and a client with a
  cursor from before the restore takes the snapshot path.
restore with a master key whose fingerprint differs from the manifest's;
  verify the refusal names the mismatch and nothing was written.
```
