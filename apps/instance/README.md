# Lumia instance

The Elixir server that one Lumia deployment runs. A Phoenix application with no HTML surface: clients talk to it over the gateway and the API defined in [docs/protocol.md](../../docs/protocol.md). Read [docs/architecture.md](../../docs/architecture.md) before changing anything here.

## Status

Early. The event-log write path exists and is measured; the gateway, API, identity, and E2EE delivery service do not exist yet. See architecture section 18 for the order.

## Requirements

- Elixir 1.20 and OTP 29. The repo root has a `.tool-versions`, so `mise install` or `asdf install` gets the exact versions.
- Postgres 18. Either a local server or `docker compose up -d` in this directory, which works with Docker Desktop and OrbStack.

Dev and test connect to `postgres:postgres@localhost` by default. Set `DATABASE_URL` to override.

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
- Elixir 1.20 types are checked at compile time; no Dialyzer, no `@spec` on private functions. Type warnings are errors.
- Phoenix.PubSub over Erlang distribution is the only coordination between nodes. No Redis, no broker.

## Spikes

Measurements that shaped the design live in [docs/spikes/](../../docs/spikes/). To rerun the event-log spike:

```sh
mix lumia.spike.events --channels 1,100 --writers 50 --duration 15
```

It writes to the dev database; `mix ecto.reset` clears it.
