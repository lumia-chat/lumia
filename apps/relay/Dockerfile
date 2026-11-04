# The Lumia push relay image: the Elixir release alone (no database, no
# assets). Build from this directory:
#
#   docker build -t lumia/relay apps/relay
#
# Credentials come from the environment at boot (config/runtime.exs); the
# APNs key and the FCM service account may be passed inline (APNS_KEY,
# FCM_SERVICE_ACCOUNT) where a platform offers secrets as variables only.

FROM hexpm/elixir:1.20.4-erlang-29.1.2-debian-trixie-20261027-slim AS build
RUN apt-get update -qq && apt-get install -y -qq --no-install-recommends build-essential git \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
ENV MIX_ENV=prod
RUN mix local.hex --force && mix local.rebar --force
COPY mix.exs mix.lock ./
RUN mix deps.get --only prod && mix deps.compile
COPY config config
COPY lib lib
RUN mix compile && mix release

FROM debian:trixie-slim
RUN apt-get update -qq && apt-get install -y -qq --no-install-recommends libstdc++6 openssl libncurses6 locales ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && sed -i '/en_US.UTF-8/s/^# //g' /etc/locale.gen && locale-gen
ENV LANG=en_US.UTF-8 LANGUAGE=en_US:en LC_ALL=en_US.UTF-8 MIX_ENV=prod
RUN useradd --create-home --uid 1000 lumia
WORKDIR /app
COPY --from=build --chown=lumia:lumia /app/_build/prod/rel/lumia_relay ./
USER lumia
EXPOSE 4100
CMD ["bin/lumia_relay", "start"]
