#!/bin/sh
###############################################################################
# DO NOT RUN THIS DIRECTLY TO UPDATE THE PROTOCOL HASH,
# RUN reproduce/reproduce.sh OR dump_all_bins.sh INSTEAD.
###############################################################################

set -eu

OUTPUT_FILE="protocol_hash.txt"
printf '🔧  Generating hash of the *local* Rust crates actually linked into this binary …\n'

###############################################################################
# 1. Metadata (locked graph)
###############################################################################
META=$(cargo metadata --format-version 1 --locked)

###############################################################################
# 2. jq – keep only local crates that appear in the resolved build graph
###############################################################################
CRATE_MANIFESTS=$(printf '%s' "$META" | jq -r '
    (reduce .resolve.nodes[].id as $i ({}; .[$i] = true)) as $used
  | .packages[]
  | select(.source == null and $used[.id])
  | .manifest_path
')

[ -n "$CRATE_MANIFESTS" ] || { echo "❌  No local crates in this build." >&2; exit 1; }

###############################################################################
# 3. Collect source files; sort & de-duplicate
###############################################################################
tmp_list=$(mktemp)
for manifest in $CRATE_MANIFESTS; do
  dir=$(dirname "$manifest")

  # list *only* tracked files inside the allowed paths of this crate
  git ls-files -z -- \
      "$dir/src" \
      "$dir/examples" \
      "$dir/benches" \
      "$dir/tests" \
      "$dir/build.rs" \
      "$dir/Cargo.toml" \
      "$dir/Cargo.lock" \
    | tr '\0' '\n'
done \
  | grep -v '/target/' \
  | grep -vE '\.(bin|elf|text|sh|md|txt|out)$' \
  | sort -u > "$tmp_list"

###############################################################################
# 4. Hash file contents
###############################################################################
hash_cmd=""
if   command -v sha256sum >/dev/null 2>&1; then hash_cmd="sha256sum"
elif command -v shasum    >/dev/null 2>&1; then hash_cmd="shasum -a 256"
else
  echo "❌  Need sha256sum (GNU) or shasum (BSD) on PATH." >&2; exit 1
fi

HASH=$(while IFS= read -r f; do cat "$f"; done <"$tmp_list" \
      | $hash_cmd | awk '{print $1}')

printf '%s\n' "$HASH" > "$OUTPUT_FILE"
printf '✅  Local-code hash: %s\n' "$HASH"

rm -f "$tmp_list"
