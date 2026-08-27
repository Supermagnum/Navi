# pbf-decode-bench

Isolated PBF decode wall-time / peak-RSS probe used to compare `osmpbf`
(default zlib), `osmpbf`+`zlib-ng`, and `fast-osmpbf`. Not part of the root
Cargo workspace. Production Navi code stays on default workspace `osmpbf`;
this crate's optional features are for local A/B only.

```bash
cargo run --release --manifest-path tools/pbf-decode-bench/Cargo.toml -- \
  /path/to/region.osm.pbf
# Optional backends (local only; not used by the app):
cargo run --release --manifest-path tools/pbf-decode-bench/Cargo.toml \
  --no-default-features --features osmpbf-zlib-ng -- /path/to/region.osm.pbf
```
