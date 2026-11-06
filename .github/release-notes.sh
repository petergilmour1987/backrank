#!/usr/bin/env bash
# Prints a version's section of CHANGELOG.md, and fails if it has no notes. Headings alone,
# such as `### Added`, aren't notes.
set -euo pipefail

version=$1
notes=$(awk -v heading="## [$version]" '
  index($0, heading) == 1 { found = 1; next }
  /^## \[/ { found = 0 }
  found
' CHANGELOG.md)

# Not `grep -q`: stopping at the first match can kill `grep -v` with SIGPIPE, and pipefail would
# then reject a long section. The error goes to stderr because callers redirect stdout.
if ! grep -v '^#' <<< "$notes" | grep '[^[:space:]]' > /dev/null; then
  echo "::error::CHANGELOG.md has no notes for $version" >&2
  exit 1
fi
printf '%s\n' "$notes"
