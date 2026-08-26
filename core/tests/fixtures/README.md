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
# Sizes stay under a few MiB so they can live in git (unlike
# core/target/integration-fixtures/*.pbf which are gitignored).
