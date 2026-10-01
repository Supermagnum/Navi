# navi-hamlib-sys

Minimal hand-written `extern "C"` bindings for [Hamlib](https://github.com/Hamlib/Hamlib)
used by Navi CAT control.

## Licensing

Hamlib is **LGPL-2.1+**. This crate only exposes a small subset of the C API and
is intended to be used with a **dynamically linked** `libhamlib.so` that users
can replace (LGPL replacement requirement).

Navi itself is GPL-3.0-or-later. Dynamically linking LGPL-2.1+ Hamlib into a
GPL-3.0-or-later application is compatible. Do **not** statically link Hamlib
into Navi binaries.

## Features

| Feature | Effect |
|---|---|
| (default) | Types + stub implementations; no link to `libhamlib.so` |
| `link-hamlib` | Link `libhamlib` and call the real C API |

## Safety

This crate never binds `rig_set_ptt`. PTT remains a read-only interlock via
`rig_get_ptt`. Callers must not key the transmitter from software.
