# The Lumia instance image (docs/specs/deployment.md): the Elixir release
# with the web UI built in, and the Postgres client tools for
# `bin/lumia-admin backup` and `restore`. Build from the repository root:
#
#   docker build -f apps/instance/Dockerfile -t lumia/instance .

# ---- The web UI: the Rust core to wasm, then the Vite build ---------------------
FROM rust:1.96.1-slim-bookworm AS web
RUN apt-get update -qq && apt-get install -y -qq --no-install-recommends curl unzip ca-certificates clang llvm \
    && rm -rf /var/lib/apt/lists/* \
    && curl -fsSL https://bun.sh/install | BUN_INSTALL=/usr/local bash -s "bun-v1.3.11" \
    && rustup target add wasm32-unknown-unknown \
    && rustup component add llvm-tools \
    && cargo install wasm-bindgen-cli --version 0.2.130 --locked
WORKDIR /src
COPY proto proto
COPY assets assets
COPY apps/core apps/core
COPY apps/web apps/web
# apps/core's rust-toolchain.toml names the pinned toolchain this image carries.
RUN cd apps/web && bun install --frozen-lockfile && bun run build:instance

# ---- The release --------------------------------------------------------------
FROM hexpm/elixir:1.20.4-erlang-29.1.2-debian-trixie-20261027-slim AS build
RUN apt-get update -qq && apt-get install -y -qq --no-install-recommends build-essential git \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
ENV MIX_ENV=prod
# The Rust toolchain of the stage above, for the instance's one native
# library (native/lumia_media, built by Rustler as the instance compiles).
# native/lumia_media/rust-toolchain.toml names the version that image carries.
COPY --from=web /usr/local/rustup /usr/local/rustup
COPY --from=web /usr/local/cargo /usr/local/cargo
ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
RUN mix local.hex --force && mix local.rebar --force
COPY apps/instance/mix.exs apps/instance/mix.lock ./
RUN mix deps.get --only prod && mix deps.compile
COPY apps/instance/config config
COPY apps/instance/native native
COPY apps/instance/lib lib
COPY apps/instance/priv priv
COPY apps/instance/rel rel
COPY --from=web /src/apps/instance/priv/static/ui priv/static/ui
RUN mix compile && mix release

# ---- The runtime ---------------------------------------------------------------
FROM debian:trixie-slim
RUN apt-get update -qq && apt-get install -y -qq --no-install-recommends libstdc++6 openssl libncurses6 locales ca-certificates curl gnupg lsb-release \
    && install -d /usr/share/postgresql-common/pgdg \
    && curl -fsSL -o /usr/share/postgresql-common/pgdg/apt.postgresql.org.asc https://www.postgresql.org/media/keys/ACCC4CF8.asc \
    && echo "deb [signed-by=/usr/share/postgresql-common/pgdg/apt.postgresql.org.asc] https://apt.postgresql.org/pub/repos/apt $(lsb_release -cs)-pgdg main" > /etc/apt/sources.list.d/pgdg.list \
    && apt-get update -qq && apt-get install -y -qq --no-install-recommends postgresql-client-18 \
    && apt-get purge -y -qq gnupg lsb-release && apt-get autoremove -y -qq && rm -rf /var/lib/apt/lists/* \
    && sed -i '/en_US.UTF-8/s/^# //g' /etc/locale.gen && locale-gen
ENV LANG=en_US.UTF-8 LANGUAGE=en_US:en LC_ALL=en_US.UTF-8 MIX_ENV=prod PHX_SERVER=true
# The backup directory and the blob store's (LUMIA_BLOB_STORE=fs:/data/blobs)
# that the compose example mounts volumes on; a named volume takes the
# image's ownership on first use.
RUN useradd --create-home --uid 1000 lumia && install -d -o lumia -g lumia /backups /data/blobs
WORKDIR /app
COPY --from=build --chown=lumia:lumia /app/_build/prod/rel/lumia ./
USER lumia
EXPOSE 4000
# Migrations first (behind Postgres's advisory lock), then the server.
CMD ["sh", "-c", "bin/lumia-admin migrate && exec bin/lumia start"]
