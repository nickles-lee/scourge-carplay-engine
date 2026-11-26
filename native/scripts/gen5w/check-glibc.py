#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 The SCOURGE Project contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# Additional terms: see NOTICE
"""Fail closed when a Gen5W artifact needs a newer glibc than the unit.

The unit runs glibc 2.25 (`libc-2.25.so`) and GStreamer 1.8.3. This check is
deliberately independent of the Rust build: it inspects the final ELF and
rejects both a newer symbol version and an unexpected shared-library
dependency. A developer host build is not evidence of target compatibility.
"""

import argparse
import re
import subprocess
import sys

DEFAULT_MAX = "2.25"
ALLOWED_LIBS = {
    "libc.so.6",
    "ld-linux-x86-64.so.2",
    "libm.so.6",
    "libdl.so.2",
    "libpthread.so.0",
    "librt.so.1",
    "libutil.so.1",
}


def version(text):
    return tuple(int(part) for part in text.split("."))


def readelf(*args):
    return subprocess.run(
        ["readelf", "-W", *args], check=True, text=True, capture_output=True
    ).stdout


def audit(path, maximum):
    versions = {
        name
        for name in re.findall(r"Name: (GLIBC_[0-9.]+)", readelf("--version-info", path))
    }
    too_new = sorted(
        (name for name in versions if version(name.removeprefix("GLIBC_")) > maximum),
        key=lambda name: version(name.removeprefix("GLIBC_")),
    )
    needed = sorted(
        set(re.findall(r"\(NEEDED\).*\[(.+?)\]", readelf("-d", path)))
    )
    unexpected = [lib for lib in needed if lib not in ALLOWED_LIBS]
    interp = re.search(
        r"Requesting program interpreter: (.+?)\]", readelf("-l", path)
    )
    interp = interp.group(1) if interp else None
    highest = max(
        (name for name in versions), key=lambda name: version(name.removeprefix("GLIBC_")), default="none"
    )
    print(f"{path}:")
    print(f"  glibc_max={highest}  needed={needed}  interp={interp}")
    errors = []
    if not versions:
        errors.append("no GLIBC symbol versions (static or non-glibc artifact)")
    if interp is None:
        errors.append("no program interpreter (static or non-glibc artifact)")
    elif interp not in ("/lib64/ld-linux-x86-64.so.2", "/lib/ld-linux-x86-64.so.2"):
        errors.append("unexpected interpreter: %s" % interp)
    if "libc.so.6" not in needed:
        errors.append("does not link the unit's libc.so.6")
    if too_new:
        errors.append("symbols newer than GLIBC_%s: %s" % (maximum, ", ".join(too_new)))
    if unexpected:
        errors.append("unexpected shared libraries: %s" % ", ".join(unexpected))
    for error in errors:
        print(f"  FAIL: {error}", file=sys.stderr)
    return not errors


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--max", default=DEFAULT_MAX)
    parser.add_argument("paths", nargs="+")
    args = parser.parse_args()
    maximum = version(args.max)
    ok = all(audit(path, maximum) for path in args.paths)
    print("PASS: target glibc gate" if ok else "FAIL: target glibc gate", file=sys.stderr)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
