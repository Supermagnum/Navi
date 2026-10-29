//! Match repeaters across OSM, RadioID, and local files.
//!
//! A match is the same callsign, an output frequency within 12.5 kHz, and a
//! position within 1 km. Mode is recorded when it differs. It does not block
//! a match.
//!
//! Norway: an OSM row keeps its position. RadioID and a file may replace
//! output frequency, access tone, or shift on that row when the incoming
//! value is known and different. A repeater with no OSM row is added with its
//! own position.
//!
//! Outside Norway: when RadioID and a file match, the RadioID row keeps
//! position, frequency, tone, and shift. A field RadioID does not have is
//! filled from the file. OSM outside Norway is left as its own row. No
//! precedence is applied to it.

use crate::repeater::{AccessTone, RepeaterDb, RepeaterSite, RepeaterSource};

pub const MATCH_DISTANCE_KM: f64 = 1.0;
pub const MATCH_FREQ_MHZ: f64 = 0.0125;

#[derive(Debug, Clone, PartialEq)]
pub enum MergeOutcome {
    /// Inserted as its own row.
    Added,
    /// Folded into an existing row. `fields` names what changed.
    Merged { fields: Vec<String> },
    /// Same callsign and frequency, but every OSM site is farther than 1 km.
    PositionDiffers { distance_km: f64 },
}

pub fn merge_incoming(db: &RepeaterDb, incoming: &RepeaterSite) -> MergeOutcome {
    if incoming.source == RepeaterSource::Osm {
        let _ = db.upsert_site(incoming);
        return MergeOutcome::Added;
    }
    let sites = db.list_sites();
    let same_call: Vec<&RepeaterSite> = sites
        .iter()
        .filter(|s| s.callsign.eq_ignore_ascii_case(&incoming.callsign))
        .filter(|s| s.id != incoming.id)
        .collect();

    if let Some(osm) = norway_osm_match(&same_call, incoming) {
        let (updated, fields) = apply_norway_override(osm, incoming);
        let _ = db.upsert_site(&updated);
        let _ = db.delete_site(&incoming.id);
        return MergeOutcome::Merged { fields };
    }

    if let Some(km) = osm_position_differs(&same_call, incoming) {
        let mut row = incoming.clone();
        let note = format!("position differs from OSM ({km:.1} km)");
        row.conflict = true;
        row.conflict_note = Some(match row.conflict_note {
            Some(existing) => format!("{existing}; {note}"),
            None => note,
        });
        let _ = db.upsert_site(&row);
        return MergeOutcome::PositionDiffers { distance_km: km };
    }

    if country_is_norway(incoming) || same_call.iter().any(|s| country_is_norway(s)) {
        // Norway, no OSM row for this site. Keep this source's position.
        // Fill only unknown fields from the other non-OSM row, and drop the
        // duplicate so the sheet does not list the same repeater twice.
        if let Some(other) = non_osm_match(&same_call, incoming) {
            let (kept, fields) = fill_unknowns(other, incoming);
            let _ = db.upsert_site(&kept);
            if kept.id != incoming.id {
                let _ = db.delete_site(&incoming.id);
            }
            if other.id != kept.id {
                let _ = db.delete_site(&other.id);
            }
            return MergeOutcome::Merged { fields };
        }
        let _ = db.upsert_site(incoming);
        return MergeOutcome::Added;
    }

    if osm_outside_norway(&same_call, incoming) {
        let mut row = incoming.clone();
        let note = "OSM outside Norway; no precedence, kept as a separate source";
        row.conflict_note = Some(match row.conflict_note {
            Some(existing) => format!("{existing}; {note}"),
            None => note.to_string(),
        });
        let _ = db.upsert_site(&row);
        return MergeOutcome::Added;
    }

    if let Some((radio, file)) = radioid_file_pair(&same_call, incoming) {
        let (updated, fields) = apply_radioid_wins(radio, file);
        let _ = db.upsert_site(&updated);
        if file.id != updated.id {
            let _ = db.delete_site(&file.id);
        }
        return MergeOutcome::Merged { fields };
    }

    let _ = db.upsert_site(incoming);
    MergeOutcome::Added
}

fn osm_in_norway(osm: &RepeaterSite, incoming: &RepeaterSite) -> bool {
    if country_is_norway(osm) {
        return true;
    }
    let blank = osm
        .country
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .is_empty();
    blank && country_is_norway(incoming)
}

fn norway_osm_match<'a>(
    sites: &[&'a RepeaterSite],
    incoming: &RepeaterSite,
) -> Option<&'a RepeaterSite> {
    sites.iter().copied().find(|s| {
        s.source == RepeaterSource::Osm
            && osm_in_norway(s, incoming)
            && freq_close(s, incoming)
            && pos_close(s, incoming)
    })
}

fn osm_position_differs(sites: &[&RepeaterSite], incoming: &RepeaterSite) -> Option<f64> {
    let osm: Vec<&RepeaterSite> = sites
        .iter()
        .copied()
        .filter(|s| {
            s.source == RepeaterSource::Osm && osm_in_norway(s, incoming) && freq_close(s, incoming)
        })
        .collect();
    if osm.is_empty() {
        return None;
    }
    if osm.iter().any(|s| pos_close(s, incoming)) {
        return None;
    }
    let mut best = f64::MAX;
    for s in osm {
        if let Some(d) = distance(s, incoming) {
            best = best.min(d);
        }
    }
    (best < f64::MAX).then_some(best)
}

fn osm_outside_norway(sites: &[&RepeaterSite], incoming: &RepeaterSite) -> bool {
    sites.iter().any(|s| {
        s.source == RepeaterSource::Osm
            && !country_is_norway(s)
            && freq_close(s, incoming)
            && pos_close(s, incoming)
    })
}

fn non_osm_match<'a>(
    sites: &[&'a RepeaterSite],
    incoming: &RepeaterSite,
) -> Option<&'a RepeaterSite> {
    sites.iter().copied().find(|s| {
        s.source != RepeaterSource::Osm && freq_close(s, incoming) && pos_close(s, incoming)
    })
}

fn radioid_file_pair<'a>(
    sites: &[&'a RepeaterSite],
    incoming: &'a RepeaterSite,
) -> Option<(&'a RepeaterSite, &'a RepeaterSite)> {
    if incoming.source == RepeaterSource::RadioId {
        let file = sites.iter().copied().find(|s| {
            s.source == RepeaterSource::RepeaterBook
                && freq_close(s, incoming)
                && pos_close(s, incoming)
        })?;
        return Some((incoming, file));
    }
    if incoming.source == RepeaterSource::RepeaterBook {
        let radio = sites.iter().copied().find(|s| {
            s.source == RepeaterSource::RadioId && freq_close(s, incoming) && pos_close(s, incoming)
        })?;
        return Some((radio, incoming));
    }
    None
}

fn apply_norway_override(
    osm: &RepeaterSite,
    incoming: &RepeaterSite,
) -> (RepeaterSite, Vec<String>) {
    let mut updated = osm.clone();
    let mut fields = Vec::new();
    if (osm.freq_out_mhz - incoming.freq_out_mhz).abs() > 1e-6
        && (osm.freq_out_mhz - incoming.freq_out_mhz).abs() <= MATCH_FREQ_MHZ
    {
        updated.freq_out_mhz = incoming.freq_out_mhz;
        fields.push("frequency".into());
    }
    if access_known_and_different(osm, incoming) {
        copy_access(&mut updated, incoming);
        fields.push(access_field_name(incoming));
    }
    if let Some(shift) = incoming.shift_mhz {
        match osm.shift_mhz {
            Some(old) if (old - shift).abs() <= 0.001 => {}
            _ => {
                updated.shift_mhz = Some(shift);
                fields.push("shift".into());
            }
        }
    }
    if modes_differ(osm, incoming) {
        fields.push(format!(
            "mode (reported, OSM kept): {}",
            incoming.modulation
        ));
    }
    if !fields.is_empty() {
        updated.conflict = true;
        updated.conflict_note = Some(format!(
            "override ({}) from {}",
            fields.join(", "),
            incoming.attribution.as_deref().unwrap_or("other source")
        ));
    }
    if updated.network_id.is_none() {
        updated.network_id = incoming.network_id.clone();
    }
    (updated, fields)
}

fn apply_radioid_wins(radio: &RepeaterSite, file: &RepeaterSite) -> (RepeaterSite, Vec<String>) {
    let mut updated = radio.clone();
    let mut fields = Vec::new();
    if (radio.freq_out_mhz - file.freq_out_mhz).abs() > 1e-6 {
        fields.push("frequency".into());
    }
    if access_known_and_different(radio, file) {
        fields.push(access_field_name(file));
    }
    if let (Some(a), Some(b)) = (radio.shift_mhz, file.shift_mhz) {
        if (a - b).abs() > 0.001 {
            fields.push("shift".into());
        }
    }
    if updated.shift_mhz.is_none() {
        updated.shift_mhz = file.shift_mhz;
    }
    if updated.access == AccessTone::Unknown && file.access != AccessTone::Unknown {
        copy_access(&mut updated, file);
    }
    if updated.network_id.is_none() {
        updated.network_id = file.network_id.clone();
    }
    if modes_differ(radio, file) {
        fields.push(format!(
            "mode (reported, RadioID kept): {}",
            file.modulation
        ));
    }
    if !fields.is_empty() {
        updated.conflict_note = Some(format!(
            "RadioID kept; file differs ({})",
            fields.join(", ")
        ));
    }
    (updated, fields)
}

fn fill_unknowns(existing: &RepeaterSite, incoming: &RepeaterSite) -> (RepeaterSite, Vec<String>) {
    // Prefer the RadioID row as the stored id when one of the two is RadioID.
    let (base, extra) = if incoming.source == RepeaterSource::RadioId {
        (incoming, existing)
    } else if existing.source == RepeaterSource::RadioId {
        (existing, incoming)
    } else if incoming.source == RepeaterSource::RepeaterBook {
        (existing, incoming)
    } else {
        (existing, incoming)
    };
    let mut kept = base.clone();
    let mut fields = Vec::new();
    if kept.shift_mhz.is_none() && extra.shift_mhz.is_some() {
        kept.shift_mhz = extra.shift_mhz;
        fields.push("shift".into());
    }
    if kept.access == AccessTone::Unknown && extra.access != AccessTone::Unknown {
        copy_access(&mut kept, extra);
        fields.push(access_field_name(extra));
    }
    if kept.network_id.is_none() && extra.network_id.is_some() {
        kept.network_id = extra.network_id.clone();
        fields.push("network".into());
    }
    (kept, fields)
}

fn copy_access(dst: &mut RepeaterSite, src: &RepeaterSite) {
    dst.access = src.access;
    dst.ctcss_hz = src.ctcss_hz;
    dst.dcs_code = src.dcs_code;
}

fn access_known_and_different(existing: &RepeaterSite, incoming: &RepeaterSite) -> bool {
    if incoming.access == AccessTone::Unknown {
        return false;
    }
    if incoming.access != existing.access {
        return true;
    }
    match incoming.access {
        AccessTone::Ctcss => match (existing.ctcss_hz, incoming.ctcss_hz) {
            (Some(a), Some(b)) => (a - b).abs() > 0.05,
            (None, Some(_)) => true,
            _ => false,
        },
        AccessTone::Dcs => existing.dcs_code != incoming.dcs_code && incoming.dcs_code.is_some(),
        _ => false,
    }
}

fn access_field_name(site: &RepeaterSite) -> String {
    match site.access {
        AccessTone::Ctcss => "ctcss".into(),
        AccessTone::Dcs => "dcs".into(),
        AccessTone::ToneBurst1750 => "1750 Hz burst".into(),
        AccessTone::Carrier => "carrier".into(),
        AccessTone::Unknown => "access".into(),
    }
}

fn is_digital_mode(mode: &str) -> bool {
    let upper = mode.to_ascii_uppercase();
    upper.contains("DMR")
        || upper.contains("DSTAR")
        || upper.contains("D-STAR")
        || upper.contains("C4FM")
        || upper.contains("P25")
        || upper.contains("YSF")
}

fn modes_differ(a: &RepeaterSite, b: &RepeaterSite) -> bool {
    let left = a.modulation.trim();
    let right = b.modulation.trim();
    !left.is_empty() && !right.is_empty() && is_digital_mode(left) != is_digital_mode(right)
}

fn freq_close(a: &RepeaterSite, b: &RepeaterSite) -> bool {
    (a.freq_out_mhz - b.freq_out_mhz).abs() <= MATCH_FREQ_MHZ
}

fn pos_close(a: &RepeaterSite, b: &RepeaterSite) -> bool {
    distance(a, b).is_some_and(|d| d <= MATCH_DISTANCE_KM)
}

fn distance(a: &RepeaterSite, b: &RepeaterSite) -> Option<f64> {
    match (a.lat, a.lon, b.lat, b.lon) {
        (Some(lat1), Some(lon1), Some(lat2), Some(lon2)) => {
            Some(haversine_km(lat1, lon1, lat2, lon2))
        }
        _ => None,
    }
}

pub fn country_is_norway(site: &RepeaterSite) -> bool {
    matches!(
        site.country.as_deref().map(str::trim),
        Some("NO") | Some("no") | Some("Norway") | Some("norway")
    )
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
