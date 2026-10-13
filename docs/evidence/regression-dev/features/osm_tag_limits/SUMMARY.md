# OSM vehicle-limit tag proof (host)

Source: `cargo test -p driver-break-core --test innlandet_real_world_limits` — **5/5 PASS**.

| OSM tag | Way / place | Limit | Vehicle that detours | Evidence |
|---|---|---|---|---|
| maxweight | Atna hengebru (323447948) | 7.5 t | total_weight > 7.5 t | pack_path_atna_maxweight_finds_detour |
| maxlength | Atna hengebru | 12.4 m | length > 12.4 m | pack_path_atna_maxlength_finds_detour |
| maxwidth | Stai bru (34106197) | 3.5 m | width 4.0 m | pack_path_stai_maxwidth_and_maxaxleload_enforced |
| maxaxleload | Stai bru | 6 t | axle 8 t | same |
| maxbogieweight | Liabrue | 7.5 t | bogie over limit | pack_path_liabrue_maxbogieweight_enforced |
| maxheight | Fokholgutua | (pack) | height over | pack_path_fokholgutua_maxheight_finds_detour |

T6 routing width: **`width_m=2.297`** (mirrors) is what `VehicleLimits` compares to OSM `maxwidth`. Body width 1.904 is not a separate routing field. Unit proof: `vehicle_filtered_snap_uses_o1_incident_not_full_edge_scan` uses MobileHome width 2.297 vs maxwidth 2.0.

Bridge avoidance: no separate `avoid_bridges` toggle — clearance tags on bridge ways force detours when vehicle limits exceed them (table above).
