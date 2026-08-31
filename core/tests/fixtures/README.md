# Checked-in mini OSM extracts for per-PR Rust regression guards.
#
# Regenerated from regional Geofabrik extracts with:
#
#   python3 scripts/cut-corridor-extract.py \
#     --src core/target/integration-fixtures/ostlandet-latest.osm.pbf \
#     --dst core/tests/fixtures/motor-access-hamar-gjovik.osm.pbf \
#     --bbox 11.0740,60.7895,11.0795,60.7945 \
#     --bbox 10.6845,60.7755,10.6920,60.7808
#
#   python3 scripts/cut-corridor-extract.py \
#     --src core/target/integration-fixtures/hedmark-latest.osm.pbf \
#     --dst core/tests/fixtures/atnbrufossen-wetland.osm.pbf \
#     --bbox 10.05,61.70,10.45,61.95
#
# Innlandet vehicle-limit corridors (see `innlandet_real_world_limits.rs` and
# `graph_pack_v5_to_v6_regen.rs`):
#
#   python3 scripts/cut-corridor-extract.py \
#     --src core/target/integration-fixtures/hedmark-latest.osm.pbf \
#     --dst core/tests/fixtures/fokholgutua-maxheight.osm.pbf \
#     --bbox 11.150,60.710,11.220,60.750
#
#   python3 scripts/cut-corridor-extract.py \
#     --src core/target/integration-fixtures/hedmark-latest.osm.pbf \
#     --dst core/tests/fixtures/atna-hengebru-limits.osm.pbf \
#     --bbox 10.800,61.710,10.860,61.750
#
#   python3 scripts/cut-corridor-extract.py \
#     --src core/target/integration-fixtures/hedmark-latest.osm.pbf \
#     --dst core/tests/fixtures/stai-bru-limits.osm.pbf \
#     --bbox 11.000,61.460,11.120,61.540
#
#   python3 scripts/cut-corridor-extract.py \
#     --src core/target/integration-fixtures/oppland-latest.osm.pbf \
#     --dst core/tests/fixtures/liabrue-bogie-limits.osm.pbf \
#     --bbox 8.620,61.820,8.760,61.900
#
# Parallel-edge geometry (Budorvegen secondary vs service loop):
#
#   python3 scripts/cut-corridor-extract.py \
#     --src core/target/integration-fixtures/ostlandet-latest.osm.pbf \
#     --dst core/tests/fixtures/budorvegen-service-detour.osm.pbf \
#     --bbox 11.30,60.878,11.32,60.890
#
# Sizes stay under a few MiB so they can live in git (unlike
# core/target/integration-fixtures/*.pbf which are gitignored).
