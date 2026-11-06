# Lumia instance

The Elixir server that one Lumia deployment runs. A Phoenix application whose only HTML is the web UI it can serve: clients talk to it over the gateway and the API defined in [docs/protocol.md](../../docs/protocol.md). Read [docs/architecture.md](../../docs/architecture.md) before changing anything here.

## Status

Early. The event log, gateway, HTTP API, sessions, authorization, identity, and the MLS delivery service for personal DMs exist and are tested; presence, invites, push, and multi-node operation do not. [docs/plan.md](../../docs/plan.md) tracks what is done and what is next; architecture section 18 gives the order.

## Requirements

- Elixir 1.20 and OTP 29. The repo root has a `.tool-versions`, so `mise install` or `asdf install` gets the exact versions.
- Postgres 18. Either a local server or `docker compose up -d` in this directory, which works with Docker Desktop and OrbStack.

Dev and test connect to `postgres:postgres@localhost` by default. Set `DB_URL` to override.

If you use Postgres.app on macOS, it gates "trust" connections behind a per-process permission dialog. A server started from a non-interactive process (a script, `nohup`, an agent) cannot answer it and every connection handshake is rejected. Either approve the BEAM in Postgres.app's settings or run the server from your own terminal.

## Running

```sh
mix setup          # deps, create and migrate the database
mix test
mix phx.server     # or: iex -S mix phx.server
mix precommit      # compile with warnings as errors, format, credo --strict, test
```

## Layout

```
lib/lumia/            domain: contexts and schemas
lib/lumia_web/        endpoint, router, gateway, API controllers
bench/                spike and benchmark mix tasks; dev only, never shipped
priv/repo/migrations
test/
```

## Conventions

- Every schema uses `use Lumia.Schema`: UUIDv7 primary keys minted app-side, UUID foreign keys, microsecond UTC timestamps. Never add a database default for an id column. Never derive a time from an id.
- `Lumia.Events.append/2` is the only writer to the event-log tables. Broadcast happens after it returns, never inside it.
- Elixir 1.20 types are checked at compile time and type warnings are errors. No Dialyzer, and no `@spec`, `@type`, or `@typedoc` anywhere: nothing verifies them, so they could be wrong. Contracts go in `@doc` prose.
- Phoenix.PubSub over Erlang distribution is the only coordination between nodes. No Redis, no broker.

## Spikes

Measurements that shaped the design live in [docs/spikes/](../../docs/spikes/). To rerun the event-log spike:

```sh
mix lumia.spike.events --channels 1,100 --writers 50 --duration 15
mix lumia.spike.gateway --subscribers 200 --messages 300 --rate 50
```

Both write to the dev database; `mix ecto.reset` clears it. The gateway spike starts the endpoint on port 4799.

The load test of architecture section 17 ([docs/spikes/2026-11-06-load.md](../../docs/spikes/2026-11-06-load.md)) seeds a fixture once (10M events is 27 GB and a minute and a half) and measures every row the instance can, on port 4798:

```sh
LUMIA_CODE_RELOADER=false mix lumia.load --channels 1000 --events 10000000
LUMIA_CODE_RELOADER=false mix lumia.load --channels 1000 --events 10000000 --connections 10000
LUMIA_CODE_RELOADER=false mix lumia.load --ci   # a small fixture, the budgets checked; what CI runs
```

The reloader must be compiled out (the variable is read by `config/dev.exs`): its plug costs about 8 ms per request, which would be most of the scrollback row. The connections run wants `ulimit -n 65536`.

## The web UI

An instance serves the web app from `priv/static/ui` when a build is installed there, so its web origin is itself (architecture 11.4):

```sh
cd ../web && bun run build:instance   # builds into ../instance/priv/static/ui
```

Every path the API does not claim then renders the app's `index.html`; without a build the instance answers those paths with a plain note. The directory is not in the repo; a release build installs it.

## Deploying

[docs/specs/deployment.md](../../docs/specs/deployment.md) is the contract. In short:

```sh
docker build -f apps/instance/Dockerfile -t lumia/instance .   # from the repository root; builds the web UI in
cd apps/instance/deploy && cp .env.example .env               # SECRET_KEY_BASE, PHX_HOST, POSTGRES_PASSWORD
docker compose up -d                                           # one node; --scale instance=2 for two
docker compose exec instance bin/lumia-admin backup /backups/$(date +%F)
```

The release's own commands are `bin/lumia-admin migrate | backup <dir> | restore <dir> [--replace]` (`Lumia.Release`). Every setting is an environment variable listed in the spec's section 3; `LUMIA_METRICS=true` serves Prometheus text at `/metrics`.
