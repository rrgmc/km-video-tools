#!/usr/bin/env bash
#
# The compiler version is written down once, and this is what keeps that true.
#
#   tools/dev/check-toolchain-pin.sh   # exit 1 and name every disagreement
#
# `rust-toolchain.toml` holds the pin. One tracked file has to repeat the number, because no manifest
# format lets cargo derive it from that file -- `rust-version` in Cargo.toml -- and one must NOT
# repeat it, because it is supposed to read the file instead: .github/workflows/ci.yml. Both halves
# are checked here.
#
# The standing decision is `The Rust toolchain is pinned exactly` in docs/decisions.md. This exists
# for the reason that entry gives: a copy nothing compares is a copy that drifts, and the failure it
# produces is a build on a compiler nobody chose -- which is exactly how the pin came to be needed,
# `rust-version` having said 1.98.1 while every machine's `stable` still said 1.98.0.
#
# `task check` runs it ahead of fmt, because it reads three files and says one line.

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."
[ -f Cargo.toml ] && [ -d crates ] || { echo "${0##*/}: not at the repository root -- landed in $PWD" >&2; exit 1; }

self='check-toolchain-pin'
found=0

fail() {
  printf '%s: %s\n' "$self" "$1" >&2
  found=1
}

# -- The pin itself ------------------------------------------------------------------------------

# `channel = "1.98.1"`, and the quotes are required by the file format so this is not guesswork.
channel="$(sed -n 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' rust-toolchain.toml)"

if [ -z "$channel" ]; then
  echo "$self: rust-toolchain.toml has no channel to read" >&2
  exit 1
fi

# An exact three-part version, which is the decision rather than a formatting preference: `stable` is
# what this repository used until the pin existed, and `1.98` would let a patch release change the
# compiler without changing a tracked file.
if ! printf '%s' "$channel" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
  fail "rust-toolchain.toml pins '$channel', which is not an exact x.y.z version.
       A floating channel lets a clippy lint added upstream fail a branch that changed nothing
       relevant. See 'The Rust toolchain is pinned exactly' in docs/decisions.md."
fi

# -- The file that must carry the same number -----------------------------------------------------

declared="$(sed -n 's/^rust-version = "\([^"]*\)".*/\1/p' Cargo.toml)"
if [ -z "$declared" ]; then
  fail "Cargo.toml declares no rust-version, which should equal the pin ($channel)"
elif [ "$declared" != "$channel" ]; then
  fail "Cargo.toml says rust-version = \"$declared\" but the pin is $channel"
fi

# -- ...and the two that must not -----------------------------------------------------------------

# CI installs by running `rustup toolchain install` with no toolchain argument, which resolves
# rust-toolchain.toml. A toolchain named on that line is a second pin that a bump would not move.
if grep -Ev '^[[:space:]]*#' .github/workflows/ci.yml \
     | grep -Eq 'rustup toolchain install[[:space:]]+([0-9]|stable|beta|nightly)'; then
  fail ".github/workflows/ci.yml names a toolchain on its 'rustup toolchain install' line.
       Leave it off and rustup resolves rust-toolchain.toml, components and all."
fi

# `dtolnay/rust-toolchain` cannot read that file -- its `toolchain` input is required -- so re-adding
# the action necessarily reintroduces a copy of the number.
#
# Anchored on `uses:` rather than the bare name, because ci.yml explains why the action is not used
# and a check that forbade *naming* it would forbid its own rationale.
for wf in .github/workflows/*.yml; do
  if grep -Eq '^[[:space:]]*-?[[:space:]]*uses:[[:space:]]*dtolnay/rust-toolchain' "$wf"; then
    fail "${wf} uses dtolnay/rust-toolchain, which cannot read rust-toolchain.toml.
       Install with 'rustup toolchain install --no-self-update' instead, which resolves the file."
  fi
done

if [ "$found" -ne 0 ]; then
  printf '\n%s\n' "The pin is $channel. Bumping it means rust-toolchain.toml and Cargo.toml's
rust-version, together." >&2
  exit 1
fi

echo "$self: clean, pinned to $channel"
