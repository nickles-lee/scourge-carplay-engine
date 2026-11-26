#!/bin/sh
# SPDX-FileCopyrightText: 2026 The SCOURGE Project contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# Additional terms: see NOTICE
# TARGET: Ioniq 5 Gen5W_L (Linux), firmware 260128, glibc 2.25, x86-64.
#
# Build the protocol adapter against the unit's glibc, never against the
# builder host. A second libc on the head unit is not supported.
#
# Requires: the pinned Rust toolchain, zig (tested 0.17.1) and cargo-zigbuild
# (tested 0.24.1). Run from native/. Cargo.lock is committed; --locked fails
# when a dependency change needs resolution. Update and commit the lock in a
# separate, reviewed step.
set -eu

HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(CDPATH= cd -- "$HERE/../.." && pwd)
GLIBC_MAX=2.25
TRIPLE=x86_64-unknown-linux-gnu
TARGET=$TRIPLE.$GLIBC_MAX
TARGET_DIR=${CARGO_TARGET_DIR:-$ROOT/target}
BIN=$TARGET_DIR/$TRIPLE/release/scourge-gen5w

cd "$ROOT"
cargo zigbuild --release --locked --target "$TARGET" -p scourge-gen5w "$@"
python3 "$HERE/check-glibc.py" --max "$GLIBC_MAX" "$BIN"
printf '\nbuilt %s\n' "$BIN"
printf 'NOT car-validated: this binary has no media backend and does not own USB.\n'
