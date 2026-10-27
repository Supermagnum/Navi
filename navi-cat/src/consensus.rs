//! Consensus relocation. Default off. Never moves an OSM repeater.
//!
//! A non-OSM repeater is moved only when at least two independent sources
//! (RadioID, RepeaterBook KML, RepeaterBook CSV) agree within
//! [`CONSENSUS_RADIUS_KM`]. The target is a tall structure when its top is at
//! least [`STRUCTURE_MARGIN_M`] above the highest DEM sample in that circle.
//! Otherwise the target is that DEM sample. A missing DEM sample means no move.

use crate::repeater::{RepeaterDb, RepeaterSite, RepeaterSource};

pub const CONSENSUS_RADIUS_KM: f64 = 1.0;
pub const STRUCTURE_MARGIN_M: f64 = 15.0;

#[derive(Debug, Clone)]
pub struct StructureCandidate {
    pub id: String,
    pub lat: f64,
    pub lon: f64,
    pub ele_m: Option<f64>,
    pub height_m: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsensusAction {
    Unchanged,
    Moved,
    /// Sources disagree, a single source, or the DEM could not be sampled.
    Skipped {
        reason: String,
    },
}

pub fn revert_all(db: &RepeaterDb) -> usize {
    let mut n = 0usize;
    for mut site in db.list_sites() {
        if !site.relocated {
            continue;
        }
        if let (Some(lat), Some(lon)) = (site.original_lat, site.original_lon) {
            site.lat = Some(lat);
            site.lon = Some(lon);
        }
        site.relocated = false;
        site.relocation_note = None;
        site.original_lat = None;
        site.original_lon = None;
        let _ = db.upsert_site(&site);
        n += 1;
    }
    n
}

pub fn apply(
    db: &RepeaterDb,
    enabled: bool,
    elevation_m: &dyn Fn(f64, f64) -> Option<f64>,
    structures: &[StructureCandidate],
) -> Vec<(String, ConsensusAction)> {
    if !enabled {
        return Vec::new();
    }
    let sites = db.list_sites();
    let mut out = Vec::new();
    let mut used = vec![false; sites.len()];
    for i in 0..sites.len() {
        if used[i] || sites[i].source == RepeaterSource::Osm {
            continue;
        }
        let mut group = vec![i];
        used[i] = true;
        for j in (i + 1)..sites.len() {
            if used[j] || sites[j].source == RepeaterSource::Osm {
                continue;
            }
            if same_repeater(&sites[i], &sites[j])
                && within(&sites[i], &sites[j], CONSENSUS_RADIUS_KM * 4.0)
            {
                group.push(j);
                used[j] = true;
            }
        }
        let members: Vec<&RepeaterSite> = group.iter().map(|k| &sites[*k]).collect();
        let action = decide(&members, elevation_m, structures);
        if let ConsensusAction::Moved = &action {
            if let Some((tlat, tlon, note)) = members
                .first()
                .and_then(|_s| mean_pos(&members))
                .and_then(|(lat, lon)| target(lat, lon, elevation_m, structures).ok())
            {
                for site in &members {
                    let mut updated = (*site).clone();
                    if updated.original_lat.is_none() {
                        updated.original_lat = updated.lat;
                        updated.original_lon = updated.lon;
                    }
                    updated.lat = Some(tlat);
                    updated.lon = Some(tlon);
                    updated.relocated = true;
                    updated.relocation_note = Some(note.clone());
                    let _ = db.upsert_site(&updated);
                }
            }
        } else if let ConsensusAction::Skipped { reason } = &action {
            if reason.starts_with("sources disagree") {
                for site in &members {
                    let mut flagged = (*site).clone();
                    flagged.conflict = true;
                    flagged.conflict_note = Some(reason.clone());
                    let _ = db.upsert_site(&flagged);
                }
            }
        }
        if let Some(first) = members.first() {
            out.push((first.callsign.clone(), action));
        }
    }
    out
}

fn decide(
    members: &[&RepeaterSite],
    elevation_m: &dyn Fn(f64, f64) -> Option<f64>,
    structures: &[StructureCandidate],
) -> ConsensusAction {
    let kinds = independent_kinds(members);
    if kinds < 2 {
        return ConsensusAction::Skipped {
            reason: "single source".into(),
        };
    }
    if span_km(members) > CONSENSUS_RADIUS_KM {
        return ConsensusAction::Skipped {
            reason: format!("sources disagree ({:.1} km)", span_km(members)),
        };
    }
    let (lat, lon) = match mean_pos(members) {
        Some(p) => p,
        None => {
            return ConsensusAction::Skipped {
                reason: "no position".into(),
            };
        }
    };
    match target(lat, lon, elevation_m, structures) {
        Ok(_) => ConsensusAction::Moved,
        Err(reason) => ConsensusAction::Skipped { reason },
    }
}

fn target(
    lat: f64,
    lon: f64,
    elevation_m: &dyn Fn(f64, f64) -> Option<f64>,
    structures: &[StructureCandidate],
) -> Result<(f64, f64, String), String> {
    let mut samples = Vec::new();
    for lat_km in [-1.0_f64, -0.5, 0.0, 0.5, 1.0] {
        for lon_km in [-1.0_f64, -0.5, 0.0, 0.5, 1.0] {
            let (slat, slon) = offset_km(lat, lon, lat_km, lon_km);
            let Some(ele) = elevation_m(slat, slon) else {
                return Err("missing DEM".into());
            };
            samples.push((slat, slon, ele));
        }
    }
    let (plat, plon, pele) = samples
        .iter()
        .max_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
        .copied()
        .ok_or_else(|| "missing DEM".to_string())?;
    let mut best_structure: Option<(&StructureCandidate, f64)> = None;
    for structure in structures {
        if haversine_km(lat, lon, structure.lat, structure.lon) > CONSENSUS_RADIUS_KM {
            continue;
        }
        let base = structure
            .ele_m
            .or_else(|| elevation_m(structure.lat, structure.lon));
        let Some(base) = base else {
            continue;
        };
        let top = base + structure.height_m.unwrap_or(0.0);
        if best_structure.map(|(_, h)| top > h).unwrap_or(true) {
            best_structure = Some((structure, top));
        }
    }
    if let Some((structure, top)) = best_structure {
        if top >= pele + STRUCTURE_MARGIN_M {
            return Ok((
                structure.lat,
                structure.lon,
                format!(
                    "relocated by consensus: target structure {} elevation {top:.0} m",
                    structure.id
                ),
            ));
        }
    }
    Ok((
        plat,
        plon,
        format!("relocated by consensus: target peak elevation {pele:.0} m"),
    ))
}

fn independent_kinds(members: &[&RepeaterSite]) -> usize {
    let mut kinds = std::collections::BTreeSet::new();
    for site in members {
        let kind = match site.file_kind.as_deref() {
            Some("kml") | Some("kmz") => "kml",
            Some("csv") => "csv",
            Some("radioid") => "radioid",
            _ => match site.source {
                RepeaterSource::RadioId => "radioid",
                RepeaterSource::RepeaterBook => "kml",
                _ => continue,
            },
        };
        kinds.insert(kind);
    }
    kinds.len()
}

fn same_repeater(a: &RepeaterSite, b: &RepeaterSite) -> bool {
    a.callsign.eq_ignore_ascii_case(&b.callsign)
        && (a.freq_out_mhz - b.freq_out_mhz).abs() <= crate::resolve::MATCH_FREQ_MHZ
}

fn within(a: &RepeaterSite, b: &RepeaterSite, km: f64) -> bool {
    match (a.lat, a.lon, b.lat, b.lon) {
        (Some(a1), Some(o1), Some(a2), Some(o2)) => haversine_km(a1, o1, a2, o2) <= km,
        _ => false,
    }
}

fn span_km(members: &[&RepeaterSite]) -> f64 {
    let mut max = 0.0_f64;
    for a in members {
        for b in members {
            if let (Some(lat1), Some(lon1), Some(lat2), Some(lon2)) = (a.lat, a.lon, b.lat, b.lon) {
                max = max.max(haversine_km(lat1, lon1, lat2, lon2));
            }
        }
    }
    max
}

fn mean_pos(members: &[&RepeaterSite]) -> Option<(f64, f64)> {
    let mut n = 0.0;
    let mut lat = 0.0;
    let mut lon = 0.0;
    for site in members {
        if let (Some(slat), Some(slon)) = (site.lat, site.lon) {
            lat += slat;
            lon += slon;
            n += 1.0;
        }
    }
    (n > 0.0).then_some((lat / n, lon / n))
}

fn offset_km(lat: f64, lon: f64, north_km: f64, east_km: f64) -> (f64, f64) {
    let dlat = north_km / 111.32;
    let dlon = east_km / (111.32 * lat.to_radians().cos().max(0.2));
    (lat + dlat, lon + dlon)
}

fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0;
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

/// Structure tags read when a future bundle keeps them. The published Norway
/// bundle currently drops `man_made`, `tower:type`, `ele`, and `height`.
pub fn structure_tag_names() -> &'static [&'static str] {
    &[
        "man_made=mast",
        "man_made=tower",
        "man_made=communications_tower",
        "tower:type=communication",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repeater::{AccessTone, RepeaterDb, RepeaterSite, RepeaterSource};

    fn site(id: &str, kind: &str, lat: f64, source: RepeaterSource) -> RepeaterSite {
        RepeaterSite {
            id: id.into(),
            callsign: "SG0SZK".into(),
            lat: Some(lat),
            lon: Some(18.3852),
            freq_out_mhz: 434.5875,
            shift_mhz: Some(-2.0),
            ctcss_hz: None,
            dcs_code: None,
            color_code: None,
            modulation: "DMR".into(),
            network_id: None,
            source,
            conflict: false,
            conflict_note: None,
            distance_km: None,
            position_accurate: true,
            exclude_recommended: false,
            attribution: None,
            access: AccessTone::Unknown,
            country: Some("SE".into()),
            file_kind: Some(kind.into()),
            original_lat: None,
            original_lon: None,
            relocated: false,
            relocation_note: None,
        }
    }

    #[test]
    fn default_off_does_not_move() {
        let db = RepeaterDb::open_memory().unwrap();
        db.upsert_site(&site("a", "radioid", 59.3306, RepeaterSource::RadioId))
            .unwrap();
        db.upsert_site(&site("b", "kml", 59.3306, RepeaterSource::RepeaterBook))
            .unwrap();
        let actions = apply(&db, false, &|_, _| Some(100.0), &[]);
        assert!(actions.is_empty());
        assert!(db.list_sites().iter().all(|s| !s.relocated));
    }

    #[test]
    fn two_agreeing_sources_move_and_revert_is_exact() {
        let db = RepeaterDb::open_memory().unwrap();
        db.upsert_site(&site("a", "radioid", 59.3306, RepeaterSource::RadioId))
            .unwrap();
        db.upsert_site(&site("b", "kml", 59.3306, RepeaterSource::RepeaterBook))
            .unwrap();
        let structure = StructureCandidate {
            id: "test-mast".into(),
            lat: 59.3306,
            lon: 18.3852,
            ele_m: Some(100.0),
            height_m: Some(20.0),
        };
        let actions = apply(&db, true, &|_, _| Some(100.0), &[structure]);
        assert!(actions.iter().any(|(_, a)| *a == ConsensusAction::Moved));
        let moved = db.list_sites();
        assert!(moved.iter().all(|s| s.relocated));
        assert!(moved.iter().all(|s| s.original_lat == Some(59.3306)));
        assert_eq!(revert_all(&db), 2);
        assert!(db
            .list_sites()
            .iter()
            .all(|s| { !s.relocated && s.lat == Some(59.3306) && s.original_lat.is_none() }));
    }

    #[test]
    fn single_source_and_disagreement_and_missing_dem_do_not_move() {
        let db = RepeaterDb::open_memory().unwrap();
        db.upsert_site(&site("only", "radioid", 59.3306, RepeaterSource::RadioId))
            .unwrap();
        let actions = apply(&db, true, &|_, _| Some(100.0), &[]);
        assert!(matches!(actions[0].1, ConsensusAction::Skipped { .. }));
        assert!(!db.list_sites()[0].relocated);

        db.upsert_site(&site("far", "kml", 59.35, RepeaterSource::RepeaterBook))
            .unwrap();
        let actions = apply(&db, true, &|_, _| Some(100.0), &[]);
        assert!(actions.iter().any(|(_, a)| matches!(
            a,
            ConsensusAction::Skipped { reason } if reason.starts_with("sources disagree")
        )));
        assert!(db.list_sites().iter().all(|s| !s.relocated));
        assert!(db.list_sites().iter().any(|s| s.conflict));

        let db = RepeaterDb::open_memory().unwrap();
        db.upsert_site(&site("a", "radioid", 59.3306, RepeaterSource::RadioId))
            .unwrap();
        db.upsert_site(&site("b", "csv", 59.3306, RepeaterSource::RepeaterBook))
            .unwrap();
        let actions = apply(&db, true, &|_, _| None, &[]);
        assert!(actions.iter().any(|(_, a)| matches!(
            a,
            ConsensusAction::Skipped { reason } if reason == "missing DEM"
        )));
        assert!(db.list_sites().iter().all(|s| !s.relocated));
    }

    #[test]
    fn structure_needs_fifteen_metres_and_osm_is_never_moved() {
        let db = RepeaterDb::open_memory().unwrap();
        db.upsert_site(&site("a", "radioid", 59.3306, RepeaterSource::RadioId))
            .unwrap();
        db.upsert_site(&site("b", "kml", 59.3306, RepeaterSource::RepeaterBook))
            .unwrap();
        let short = StructureCandidate {
            id: "short".into(),
            lat: 59.331,
            lon: 18.3852,
            ele_m: Some(100.0),
            height_m: Some(10.0),
        };
        apply(&db, true, &|_, _| Some(100.0), &[short]);
        let note = db.list_sites()[0].relocation_note.clone().unwrap();
        assert!(note.contains("peak"), "{note}");
        revert_all(&db);

        let tall = StructureCandidate {
            id: "tall".into(),
            lat: 59.331,
            lon: 18.3852,
            ele_m: Some(100.0),
            height_m: Some(20.0),
        };
        apply(&db, true, &|_, _| Some(100.0), &[tall]);
        let note = db.list_sites()[0].relocation_note.clone().unwrap();
        assert!(note.contains("structure tall"), "{note}");

        let db = RepeaterDb::open_memory().unwrap();
        let mut osm = site("osm-node-1", "osm", 59.3306, RepeaterSource::Osm);
        osm.callsign = "LA5MR".into();
        let before = osm.lat;
        db.upsert_site(&osm).unwrap();
        apply(&db, true, &|_, _| Some(500.0), &[]);
        assert_eq!(db.list_sites()[0].lat, before);
        assert!(!db.list_sites()[0].relocated);
    }
}
