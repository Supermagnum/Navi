# indexing-phase-probe

Standalone host tool for measuring wetland / graph-build / keep-stats phases
during indexed-map conversion investigations. Not part of the root Cargo
workspace (own `[workspace]` in `Cargo.toml`) so it does not affect app CI deps.

```bash
cargo run --release --manifest-path tools/indexing-phase-probe/Cargo.toml -- \
  wetland-profile /path/to/region.osm.pbf
cargo run --release --manifest-path tools/indexing-phase-probe/Cargo.toml -- \
  graph-profile /path/to/region.osm.pbf /tmp/spill-dir car
cargo run --release --manifest-path tools/indexing-phase-probe/Cargo.toml -- \
  keep-stats /path/to/region.osm.pbf
cargo run --release --manifest-path tools/indexing-phase-probe/Cargo.toml -- \
  poi-barrier-profile /path/to/region.osm.pbf
```

Depends on `driver-break-core` and the production `osmpbf` crate (same as the app).
