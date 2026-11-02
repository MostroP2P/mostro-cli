#!/usr/bin/env bash
# Regenerate schemas/ from a mostro-cli binary.
#
#   MOSTRO_CLI=/path/to/mostro-cli scripts/refresh_schemas.sh
#
# Writes:
#   schemas/discovery.json              the full `--output json schema` envelope
#   schemas/<name>.input.json           input schema of every JSON-capable command
#   schemas/<name>.output.json          output schema of every JSON-capable command
#
# The command needs no node, identity or network. Review the diff afterwards:
# a changed output schema means the pages and examples may need an update.
set -euo pipefail

BIN="${MOSTRO_CLI:-mostro-cli}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/schemas"
mkdir -p "$OUT"

"$BIN" --output json schema | jq . > "$OUT/discovery.json"

jq -r '.data.commands[] | select(.output_schema != null) | .name' "$OUT/discovery.json" |
while read -r name; do
  jq --arg n "$name" '.data.commands[] | select(.name == $n) | .input_schema' \
    "$OUT/discovery.json" > "$OUT/$name.input.json"
  jq --arg n "$name" '.data.commands[] | select(.name == $n) | .output_schema' \
    "$OUT/discovery.json" > "$OUT/$name.output.json"
  echo "wrote $name"
done

echo "executable_version: $(jq -r '.data.executable_version' "$OUT/discovery.json")"
