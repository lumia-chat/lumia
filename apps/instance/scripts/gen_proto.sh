#!/usr/bin/env sh
# Regenerates lib/lumia/proto from ../../proto. Requires protoc and the
# protoc-gen-elixir escript (mix escript.install hex protobuf).
set -eu
cd "$(dirname "$0")/.."
PLUGIN="${MIX_HOME:-$HOME/.mix}/escripts/protoc-gen-elixir"
[ -x "$PLUGIN" ] || { echo "protoc-gen-elixir not found at $PLUGIN; run: mix escript.install hex protobuf" >&2; exit 1; }
INCLUDE="$(dirname "$(dirname "$(command -v protoc)")")/include"
rm -rf lib/lumia/proto && mkdir -p lib/lumia/proto
protoc -I ../../proto -I "$INCLUDE" \
  --plugin="protoc-gen-elixir=$PLUGIN" \
  --elixir_out=lib/lumia/proto \
  ../../proto/lumia/v1/*.proto
mix format "lib/lumia/proto/**/*.ex"
echo "generated $(ls lib/lumia/proto/lumia/v1 | wc -l | tr -d ' ') files"
