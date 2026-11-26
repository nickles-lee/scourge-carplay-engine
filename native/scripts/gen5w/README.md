# Gen5W build notes

Status: **WIP, offline-only.** The protocol adapter builds for the unit's
glibc and its route decoder is unit-tested. It does not open USB, AirPlay,
audio or video, and nothing here is car-validated.

## The target ABI is fixed

The car dump confirms:

| Component | Unit value | Consequence |
|---|---|---|
| glibc | 2.25 (`libc-2.25.so`) | Every artifact must stay at or below `GLIBC_2.25`. |
| GStreamer | 1.8.3 (`libgstreamer-1.0.so.0.803.0`) | Upstream's `gstreamer` 0.25 bindings (1.24 API) cannot drive it. |
| Kernel | 4.1.27-abl | Do not assume modern syscalls or io_uring. |
| Loader | `/lib64/ld-linux-x86-64.so.2` | `/lib64` is a link to the unit's `/lib`. |

A host build of `livi-core` needed `GLIBC_2.43`. It is therefore a development
artifact, not a deployable one.

## No second libc

Bundling a newer glibc under `/rw_data` was rejected:

- two loaders and two libc instances on one system; `dlopen` mixes ABIs;
- the vendor's GStreamer, Qt and projection libraries are built for 2.25;
- SMACK labels and `AT_SECURE` behavior are harder to reason about;
- security updates and crash diagnosis split across two libcs;
- it hides target incompatibility instead of fixing it.

The fork targets **one libc: the unit's 2.25**.

## Build and gate

Tested with Rust 1.100.0, zig 0.17.1 and cargo-zigbuild 0.24.1:

```sh
cd native
sh scripts/gen5w/build.sh
```

`cargo zigbuild --target x86_64-unknown-linux-gnu.2.25` links against zig's
glibc 2.25 stubs. `check-glibc.py` then reads the final ELF and rejects:

- any `GLIBC_*` symbol version above 2.25;
- any shared library outside the unit's small allowlist;
- an unexpected program interpreter.

Current result for `scourge-gen5w`: highest symbol `GLIBC_2.18`, dependencies
`libc.so.6`, `ld-linux-x86-64.so.2`, `libpthread.so.0`, `libdl.so.2`. That is
an ABI pass, not a runtime or car pass.

`Cargo.lock` is committed and the build uses `--locked`. After a deliberate
dependency change, update the lock in a separate step, review and commit it,
then rebuild. Do not let a build silently resolve new versions.

## Media is a separate process

Upstream's media crates ask GStreamer for 1.24 APIs. The unit ships 1.8.3.
Even a correctly linked Rust binary would fail at element/property level. The
port therefore keeps the protocol and the vendor pipeline apart:

```
scourge-gen5w (this crate)      vendor media shim (SCOURGE, AGPL)
  iAP2/CSM, route guidance  ──►  GStreamer 1.8 + mfxdecode/mfxsink
  bounded local IPC            ◄─  screen/audio/mic events
```

The shim must prove the vendor video sink, audio focus, microphone and input
contracts in isolation before the engine owns projection. The shim is not
written yet.

## Why a helper stack

The route guidance data comes from iAP2 CSM `0x5201`/`0x5202`/`0x5204`. LIVI's
`livi-helper-stack` already decodes it into `NavigationPatch`; this crate
normalizes that into SCOURGE's bridge contract. The same seam carries climate,
vehicle status and media metadata when those paths are implemented.

## Next

1. Build the vendor-GStreamer shim against the extracted 1.8 headers and
   libraries, with the same 2.25 audit.
2. Add a long-running engine loop and the mode switch; keep stock projection
   as the fallback.
3. Owner-present parked validation: wired video/audio/touch, then calls/Siri,
   guidance, climate readback and vehicle data.
