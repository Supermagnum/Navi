//! Offline name/address search via SQLite FTS5.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::time::Duration;

use osmpbf::Element;
use rusqlite::{params, Connection, OptionalExtension, Result as SqlResult};

use crate::download::phase_timing;
use crate::storage::Storage;

mod place_context;
mod localized_store;

pub use place_context::{format_place_display, PLACE_INDEX_SCHEMA_VERSION};
pub use localized_store::{display_name_for_locale, set_place_search_locale};

/// Place-index `kind` for OSM `building=*` + `name=*` (no amenity/shop/place).
pub const NAMED_BUILDING_KIND: &str = "building";

/// Commit SQLite/FTS inserts this often so a force-close cannot roll back the
/// entire write, and so WAL readers are not blocked for minutes.
const INSERT_COMMIT_BATCH: usize = 50_000;

/// Rows copied per PK-migrate transaction so a plan can pause within 2 s.
const MIGRATE_COPY_BATCH: i64 = 5_000;

/// Place-index build phases a pause request must be able to interrupt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexBuildPhase {
    ReadSource,
    SortPrepare,
    Insert,
    Fts,
    Commit,
    Migrate,
}

fn index_phase_tick(phase: IndexBuildPhase) {
    thread_local! {
        static LAST: std::cell::Cell<Option<IndexBuildPhase>> = const { std::cell::Cell::new(None) };
    }
    LAST.with(|last| {
        if last.get() != Some(phase) {
            last.set(Some(phase));
            log::info!(
                target: "PlaceIndexBg",
                "place_index_build phase={phase:?}"
            );
        }
    });
    let ms = crate::download::pbf_priority::test_index_phase_sleep_ms();
    if ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
}

/// Error prefix when a foreground plan pauses an in-flight index at a batch.
pub const PLACE_INDEX_PAUSED_PREFIX: &str = "place_index_paused";

fn place_index_rss_mb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines().find_map(|line| {
                let rest = line.strip_prefix("VmRSS:")?;
                rest.split_whitespace().next()?.parse::<u64>().ok()
            })
        })
        .unwrap_or(0)
        / 1024
}

fn release_index_heap() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        libc::malloc_trim(0);
    }
    #[cfg(target_os = "android")]
    {
        extern "C" {
            fn mallopt(param: libc::c_int, value: libc::c_int) -> libc::c_int;
        }
        const M_PURGE_ALL: libc::c_int = -104;
        const M_PURGE: libc::c_int = -101;
        unsafe {
            if mallopt(M_PURGE_ALL, 0) == 0 {
                mallopt(M_PURGE, 0);
            }
        }
    }
}

fn pause_index_if_plan_active(written: i64, expected: i64, peak_rss_mb: u64) -> anyhow::Result<()> {
    if !crate::download::pbf_priority::idle_pause_requested() {
        return Ok(());
    }
    release_index_heap();
    log::info!(
        target: "PlaceIndex",
        "place_index_build action=paused written={written} expected={expected} \
         peak_rss_mb={peak_rss_mb} held=committed_rows+wal"
    );
    anyhow::bail!("{PLACE_INDEX_PAUSED_PREFIX} written={written} expected={expected}")
}
/// Refresh place-index UI labels during long PBF walks so Android does not
/// appear frozen on a single "scanning ways…" / "scanning nodes…" string.
const PLACE_INDEX_PROGRESS_HEARTBEAT: usize = 25_000;

/// One writer at a time for discard + open + `load_from_pbf`. Android can
/// launch PlaceIndexBackground and RegionDownloadBackground against the same
/// DB; without this, both parse the PBF (~500 MB each) and OOM a 3.5 GB tablet.
///
/// Also serializes Kotlin `PlaceIndexReady.clearRegionRows` (via FFI
/// acquire/release) so Android framework SQLite never races rusqlite WAL
/// `-shm` (SIGBUS on deleted shm).
static PLACE_INDEX_BUILD_LOCK: Mutex<()> = Mutex::new(());

thread_local! {
    /// FFI acquire/release holds the same mutex on this thread until release.
    static PLACE_INDEX_BUILD_LOCK_TLS: std::cell::RefCell<Option<MutexGuard<'static, ()>>> =
        const { std::cell::RefCell::new(None) };
}

/// Hold while building or cache-checking a place index. Poison is recovered so
/// a panicked builder cannot deadlock later callers.
pub fn lock_place_index_build() -> MutexGuard<'static, ()> {
    PLACE_INDEX_BUILD_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Same as [`lock_place_index_build`], but if another builder already holds the
/// mutex, set a 0/6 Place-index label so the UI is not stuck on "starting…".
pub fn lock_place_index_build_with_progress() -> MutexGuard<'static, ()> {
    match PLACE_INDEX_BUILD_LOCK.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::WouldBlock) => {
            crate::download::progress::set(
                0,
                Some(6),
                "Place index: waiting for another index build…",
            );
            PLACE_INDEX_BUILD_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner())
        }
        Err(TryLockError::Poisoned(p)) => p.into_inner(),
    }
}

/// Acquire [`PLACE_INDEX_BUILD_LOCK`] on this thread for FFI / Kotlin callers.
///
/// Blocks until the lock is free (another `ensure_place_index` may be mid-build).
/// Must be paired with [`place_index_build_lock_release`] on the **same** thread.
/// Nested acquire on the same thread returns an error string rather than
/// deadlocking.
pub fn place_index_build_lock_acquire() -> Result<(), String> {
    PLACE_INDEX_BUILD_LOCK_TLS.with(|slot| {
        if slot.borrow().is_some() {
            return Err("place_index_build_lock already held on this thread".into());
        }
        let guard = lock_place_index_build_with_progress();
        *slot.borrow_mut() = Some(guard);
        Ok(())
    })
}

/// Release a prior [`place_index_build_lock_acquire`] on this thread.
pub fn place_index_build_lock_release() -> Result<(), String> {
    PLACE_INDEX_BUILD_LOCK_TLS.with(|slot| {
        if slot.borrow_mut().take().is_none() {
            return Err("place_index_build_lock not held on this thread".into());
        }
        Ok(())
    })
}

/// True when this thread currently holds the build lock via FFI acquire.
pub fn place_index_build_lock_held_on_thread() -> bool {
    PLACE_INDEX_BUILD_LOCK_TLS.with(|slot| slot.borrow().is_some())
}

/// Checkpoint / clear torn WAL sidecars before Android or Rust opens the DB.
/// Safe no-op when the file is missing or has no `-wal`.
pub fn recover_place_index_wal(index_db_path: impl AsRef<Path>) -> Result<(), String> {
    NameIndex::recover_wal_sidecars(index_db_path)
}

/// Delete one region's rows under [`PLACE_INDEX_BUILD_LOCK`] using rusqlite
/// (same stack as `ensure_place_index`). Prefer this over Android framework
/// SQLite for clears that may overlap a background index build.
pub fn clear_place_index_region_rows(
    index_db: impl AsRef<Path>,
    region_id: &str,
) -> Result<(), String> {
    let index_db = index_db.as_ref();
    let region_id = region_id.trim().trim_matches('/');
    if region_id.is_empty() {
        return Err("region_id empty".into());
    }
    if !index_db.is_file() {
        return Ok(());
    }
    let _build = lock_place_index_build_with_progress();
    let mut idx = NameIndex::open(index_db).map_err(|e| format!("open index: {e}"))?;
    idx.clear_region(region_id)
        .map_err(|e| format!("clear region: {e}"))?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct NameHit {
    pub osm_id: i64,
    pub name: String,
    pub kind: String,
    pub lat: f64,
    pub lon: f64,
    pub sub_area: String,
    pub municipality: String,
    /// Geofabrik path this row was indexed under (empty for legacy rows).
    pub region_id: String,
}

fn name_hit_from_parts(
    osm_id: i64,
    local_name: String,
    kind: String,
    lat: f64,
    lon: f64,
    sub_area: String,
    municipality: String,
    region_id: String,
    names_json: String,
) -> NameHit {
    NameHit {
        osm_id,
        name: localized_store::display_name_for_locale(&local_name, &names_json),
        kind,
        lat,
        lon,
        sub_area,
        municipality,
        region_id,
    }
}

/// True for Unicode combining marks (Mn/Me ranges used in NFD names).
fn is_combining_mark(ch: char) -> bool {
    matches!(
        ch,
        '\u{0300}'..='\u{036F}'
            | '\u{1AB0}'..='\u{1AFF}'
            | '\u{1DC0}'..='\u{1DFF}'
            | '\u{20D0}'..='\u{20FF}'
            | '\u{FE20}'..='\u{FE2F}'
    )
}

/// Map one letter to its fold key (may be 1–2 ASCII chars).
fn fold_letter(ch: char) -> &'static str {
    match ch {
        'æ' | 'ǽ' | 'ǣ' => "ae",
        'œ' => "oe",
        'ø' | 'ǿ' => "o",
        'ß' => "ss",
        'þ' => "th",
        'ł' => "l",
        'đ' | 'ð' => "d",
        'ħ' => "h",
        'ı' => "i",
        'ŋ' => "n",
        'ŧ' => "t",
        'å' | 'ä' | 'à' | 'á' | 'â' | 'ã' | 'ā' | 'ă' | 'ą' => "a",
        'ç' | 'ć' | 'č' | 'ĉ' | 'ċ' => "c",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => "e",
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'ĭ' | 'į' | 'ĩ' => "i",
        'ñ' | 'ń' | 'ņ' | 'ň' | 'ŉ' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ō' | 'ŏ' | 'ő' => "o",
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => "u",
        'ý' | 'ÿ' | 'ŷ' => "y",
        'ś' | 'š' | 'ş' | 'ŝ' | 'ș' => "s",
        'ź' | 'ż' | 'ž' => "z",
        'ğ' | 'ģ' | 'ġ' | 'ĝ' => "g",
        'ķ' | 'ĸ' => "k",
        'ľ' | 'ļ' | 'ĺ' | 'ŀ' => "l",
        'ř' | 'ŕ' | 'ŗ' => "r",
        'ť' | 'ţ' | 'ț' => "t",
        'ď' => "d",
        'ẃ' | 'ŵ' | 'ẁ' | 'ẅ' => "w",
        'ẋ' => "x",
        _ => "",
    }
}

/// NFC/NFD-insensitive, full Unicode lowercase, diacritics stripped, plus
/// æ/ø/ß/ł and similar letter expansions. `Dombås` / `Dombas` / `DOMBÅS` all
/// become `dombas`.
pub fn fold_search_key(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if is_combining_mark(ch) {
            continue;
        }
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            continue;
        }
        if ch.is_whitespace() || ch == '-' || ch == '\'' {
            out.push(ch);
            continue;
        }
        let lower = ch.to_lowercase();
        for lc in lower {
            if is_combining_mark(lc) {
                continue;
            }
            let mapped = fold_letter(lc);
            if mapped.is_empty() {
                if lc.is_alphanumeric() {
                    out.push(lc);
                }
            } else {
                out.push_str(mapped);
            }
        }
    }
    out
}

/// Extra FTS tokens so ASCII / umlaut-expanded keyboards hit the same row.
pub fn fold_name_aliases(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let folded = fold_search_key(name);
    if !folded.is_empty() && folded != name.to_lowercase() {
        out.push(folded.clone());
    }
    let mut expanded = String::new();
    for ch in name.to_lowercase().chars() {
        if is_combining_mark(ch) {
            continue;
        }
        match ch {
            'ä' => expanded.push_str("ae"),
            'ö' => expanded.push_str("oe"),
            'ü' => expanded.push_str("ue"),
            'å' => expanded.push_str("aa"),
            other => {
                let mapped = fold_letter(other);
                if mapped.is_empty() {
                    if other.is_ascii_alphanumeric() || other.is_whitespace() || other == '-' {
                        expanded.push(other);
                    } else if other.is_alphanumeric() {
                        expanded.push(other);
                    }
                } else {
                    expanded.push_str(mapped);
                }
            }
        }
    }
    if !expanded.is_empty() && expanded != folded && !out.iter().any(|a| a == &expanded) {
        out.push(expanded);
    }
    out
}

fn search_doc_with_fold_aliases(primary: &str) -> String {
    let mut parts = vec![primary.to_string()];
    let mut seen = HashSet::new();
    seen.insert(primary.to_lowercase());
    for alias in fold_name_aliases(primary) {
        if seen.insert(alias.to_lowercase()) {
            parts.push(alias);
        }
    }
    parts.join(" ")
}

fn tokenize_search(s: &str) -> Vec<String> {
    // Combining marks must stay inside the letter they belong to (NFC/NFD).
    // Splitting on U+030A turned `Dombås` (NFD) into `Domba` + `s`.
    let stripped: String = s.chars().filter(|c| !is_combining_mark(*c)).collect();
    stripped
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect()
}

fn fts_prefix_from_tokens(tokens: &[String]) -> String {
    tokens
        .iter()
        .filter_map(|t| {
            let safe = t.replace('"', "").replace('\'', " ");
            let safe = safe.trim();
            if safe.is_empty() {
                return None;
            }
            // `"1"*` expands to every token starting with 1 (10, 1867, ...):
            // hundreds of ms on a full index. House numbers match exactly.
            if safe.chars().all(|c| c.is_ascii_digit()) {
                return Some(format!("\"{safe}\""));
            }
            Some(format!("\"{safe}\"*"))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Letters that FTS may have stored in place of an ASCII query letter.
fn query_letter_expansions(ch: char) -> &'static [&'static str] {
    match ch {
        'a' => &["a", "å", "ä", "á", "æ"],
        'o' => &["o", "ø", "ö", "ó"],
        'e' => &["e", "é", "è", "æ"],
        'u' => &["u", "ü"],
        'c' => &["c", "ć", "č"],
        'l' => &["l", "ł"],
        'n' => &["n", "ñ"],
        's' => &["s", "ś", "š"],
        'z' => &["z", "ż", "ź", "ž"],
        _ => &[],
    }
}

fn letter_options(ch: char) -> Vec<String> {
    let exp = query_letter_expansions(ch);
    if exp.is_empty() {
        vec![ch.to_string()]
    } else {
        exp.iter().map(|s| (*s).to_string()).collect()
    }
}

fn prefix_stems(query: &str) -> Vec<String> {
    let folded = fold_search_key(query);
    let alnum: String = folded.chars().filter(|c| c.is_alphanumeric()).collect();
    if alnum.chars().count() < 3 {
        return Vec::new();
    }
    let take_stem = |s: &str| -> String {
        let n = s.chars().count().min(4);
        s.chars().take(n).collect()
    };
    let mut stems = Vec::new();
    let push = |stems: &mut Vec<String>, s: String| {
        if s.chars().count() >= 3 && !stems.iter().any(|e| e == &s) {
            stems.push(s);
        }
    };
    push(&mut stems, take_stem(&alnum));
    let ascii: String = alnum
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if ascii.chars().count() >= 3 {
        push(&mut stems, take_stem(&ascii));
    }
    const DIGRAPHS: &[(&str, &[&str])] = &[
        ("aa", &["å"]),
        ("ae", &["æ", "ä"]),
        ("oe", &["ø", "ö"]),
        ("ue", &["ü"]),
        ("ss", &["ß"]),
    ];
    for (from, tos) in DIGRAPHS {
        if let Some(i) = ascii.find(from) {
            if i < 4 {
                for to in *tos {
                    let replaced = ascii.replacen(from, to, 1);
                    push(&mut stems, take_stem(&replaced));
                }
            }
        }
    }
    stems
}

fn expand_stem_letters(stem: &str, cap: usize) -> Vec<String> {
    let mut acc = vec![String::new()];
    for ch in stem.chars() {
        let opts = letter_options(ch);
        let mut next = Vec::new();
        for prefix in &acc {
            for o in &opts {
                if next.len() >= cap {
                    break;
                }
                next.push(format!("{prefix}{o}"));
            }
        }
        acc = next;
        if acc.len() >= cap {
            break;
        }
    }
    acc
}

/// One FTS5 prefix MATCH for every plausible leading-letter spelling.
/// `"Kobe"* OR "Købe"* OR "Köbe"*` hits `København` on an unfolded index.
fn query_time_prefix_or_match(query: &str) -> Option<String> {
    const CAP: usize = 40;
    let mut prefixes = Vec::new();
    for stem in prefix_stems(query) {
        for variant in expand_stem_letters(&stem, CAP) {
            if variant.chars().count() < 3 {
                continue;
            }
            let q = format!("\"{variant}\"*");
            if !prefixes.iter().any(|e| e == &q) {
                prefixes.push(q);
            }
            if prefixes.len() >= CAP {
                break;
            }
        }
        if prefixes.len() >= CAP {
            break;
        }
    }
    if prefixes.is_empty() {
        None
    } else {
        Some(prefixes.join(" OR "))
    }
}

/// Spellings of the longest leading part of the first word that expands to at
/// most `cap` variants (`kobenh` -> `københ`, `kobenh`, ...).
fn anchored_first_word_variants(query: &str, cap: usize) -> Vec<String> {
    let folded = fold_search_key(query);
    let Some(first) = folded.split_whitespace().next() else {
        return Vec::new();
    };
    let mut combos = 1usize;
    let mut stem = String::new();
    for ch in first.chars().filter(|c| c.is_alphanumeric()) {
        let k = letter_options(ch).len();
        if combos * k > cap {
            break;
        }
        combos *= k;
        stem.push(ch);
    }
    if stem.chars().count() < 3 {
        return Vec::new();
    }
    expand_stem_letters(&stem, cap)
}

/// Query variants: original tokens and folded / umlaut-expanded tokens.
/// Diacritic prefixes for unfolded indexes are a separate OR MATCH.
fn search_query_variants(query: &str) -> Vec<String> {
    let mut out = Vec::new();
    let raw = tokenize_search(query);
    let raw_q = fts_prefix_from_tokens(&raw);
    if !raw_q.is_empty() {
        out.push(raw_q);
    }
    let folded: Vec<String> = raw.iter().map(|t| fold_search_key(t)).collect();
    let folded_q = fts_prefix_from_tokens(&folded);
    if !folded_q.is_empty() && !out.iter().any(|q| q == &folded_q) {
        out.push(folded_q);
    }
    for alias in fold_name_aliases(query) {
        let toks = tokenize_search(&alias);
        let q = fts_prefix_from_tokens(&toks);
        if !q.is_empty() && !out.iter().any(|e| e == &q) {
            out.push(q);
        }
    }
    let collapsed = fold_search_key(query)
        .replace("aa", "a")
        .replace("ue", "u")
        .replace("oe", "o");
    let collapsed_q = fts_prefix_from_tokens(&tokenize_search(&collapsed));
    if !collapsed_q.is_empty() && !out.iter().any(|e| e == &collapsed_q) {
        out.push(collapsed_q);
    }
    out
}

fn hit_matches_all_tokens(hit: &NameHit, query: &str) -> bool {
    let tokens = tokenize_search(&fold_search_key(query));
    if tokens.is_empty() {
        return true;
    }
    let hay = format!(
        "{} {} {}",
        fold_search_key(&hit.name),
        fold_search_key(&hit.sub_area),
        fold_search_key(&hit.municipality),
    );
    let hay_words: Vec<&str> = hay.split_whitespace().collect();
    tokens.iter().all(|tok| {
        let f = fold_search_key(tok);
        if f.is_empty() {
            return true;
        }
        let collapsed = f.replace("aa", "a").replace("ue", "u").replace("oe", "o");
        hay_words
            .iter()
            .any(|w| *w == f || *w == collapsed || w.starts_with(&f) || w.starts_with(&collapsed))
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedAddressQuery {
    pub street: String,
    pub number: Option<String>,
    pub place: Option<String>,
}

fn is_house_number_token(tok: &str) -> bool {
    let t = tok.trim();
    if t.is_empty() {
        return false;
    }
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 {
        return false;
    }
    t[digits..].chars().all(|c| c.is_ascii_alphabetic())
}

/// Parse "street number, place", "street number place", "place, street number".
pub fn parse_address_query(raw: &str) -> Option<ParsedAddressQuery> {
    let q = raw.trim();
    if q.is_empty() {
        return None;
    }
    if let Some((left, right)) = q.split_once(',') {
        let left = left.trim();
        let right = right.trim();
        if left.is_empty() || right.is_empty() {
            return None;
        }
        if let Some(addr) = parse_street_number(left) {
            return Some(ParsedAddressQuery {
                street: addr.0,
                number: addr.1,
                place: Some(right.to_string()),
            });
        }
        if let Some(addr) = parse_street_number(right) {
            return Some(ParsedAddressQuery {
                street: addr.0,
                number: addr.1,
                place: Some(left.to_string()),
            });
        }
        return None;
    }
    let toks = tokenize_search(q);
    if toks.len() < 2 {
        return None;
    }
    let num_idx = toks.iter().position(|t| is_house_number_token(t))?;
    if num_idx == 0 {
        return None;
    }
    let street = toks[..num_idx].join(" ");
    let number = Some(toks[num_idx].clone());
    let place = if num_idx + 1 < toks.len() {
        Some(toks[num_idx + 1..].join(" "))
    } else {
        None
    };
    Some(ParsedAddressQuery {
        street,
        number,
        place,
    })
}

fn parse_street_number(s: &str) -> Option<(String, Option<String>)> {
    let toks = tokenize_search(s);
    if toks.len() < 2 {
        return None;
    }
    let last = toks.last()?;
    if is_house_number_token(last) {
        return Some((toks[..toks.len() - 1].join(" "), Some(last.clone())));
    }
    None
}

fn house_number_of_display(name: &str) -> Option<String> {
    tokenize_search(name)
        .into_iter()
        .rev()
        .find(|t| is_house_number_token(t))
}

fn house_number_matches(name: &str, want: &str) -> bool {
    let a = fold_search_key(want);
    house_number_of_display(name).is_some_and(|n| fold_search_key(&n) == a)
}

/// 0 = name equals the query as typed, accents and special letters included
/// (case ignored); 1 = equal after folding; 2 = prefix; 3 = other. A query typed
/// without special letters cannot tell Åre from Are, so for it 0 and 1 are one
/// class and kind, region and distance decide.
fn search_match_class(name: &str, q_raw: &str) -> i32 {
    let name_raw_l = name.trim().to_lowercase();
    let q_raw_l = q_raw.trim().to_lowercase();
    let name_f = fold_search_key(name);
    let q_f = fold_search_key(q_raw);
    let typed_special = q_raw_l.chars().any(|c| c.is_alphabetic() && !c.is_ascii());
    if name_raw_l == q_raw_l && typed_special {
        0
    } else if name_raw_l == q_raw_l || name_f == q_f {
        1
    } else if name_raw_l.starts_with(&q_raw_l) || name_f.starts_with(&q_f) {
        2
    } else {
        3
    }
}

fn search_kind_class(kind: &str) -> i32 {
    if let Some(rest) = kind.strip_prefix("place:") {
        return match rest {
            "city" | "municipality" => 0,
            "town" => 1,
            "suburb" | "borough" | "quarter" => 2,
            "village" => 3,
            "hamlet" => 4,
            "locality" | "isolated_dwelling" => 5,
            "farm" => 7,
            _ => 6,
        };
    }
    if kind.starts_with("tourism:")
        || kind.starts_with("amenity:")
        || kind.starts_with("leisure:")
        || kind.starts_with("natural:")
    {
        return 10;
    }
    if kind.starts_with("addr:") || kind.starts_with("highway:") {
        return 20;
    }
    15
}

fn search_region_country(region_id: &str) -> &str {
    let mut parts = region_id.trim_matches('/').split('/');
    let _ = parts.next();
    parts.next().unwrap_or("")
}

fn search_haversine_km(a_lat: f64, a_lon: f64, b_lat: f64, b_lon: f64) -> i32 {
    let r = 6371.0_f64;
    let dlat = (b_lat - a_lat).to_radians();
    let dlon = (b_lon - a_lon).to_radians();
    let la1 = a_lat.to_radians();
    let la2 = b_lat.to_radians();
    let h = (dlat / 2.0).sin().powi(2) + la1.cos() * la2.cos() * (dlon / 2.0).sin().powi(2);
    let km = 2.0 * r * h.sqrt().asin();
    (km * 10.0).round() as i32
}

fn search_rank_key(
    h: &NameHit,
    q_raw: &str,
    bias: Option<(Option<f64>, Option<f64>, &str)>,
) -> (i32, i32, i32, i32, usize) {
    let match_bucket = search_match_class(&h.name, q_raw);
    let kind_c = search_kind_class(&h.kind);
    let (region_c, dist) = match bias {
        Some((lat, lon, visible)) => {
            let vis = visible.trim_matches('/');
            let region_c = if vis.is_empty() {
                1
            } else if h.region_id == vis || h.region_id.starts_with(&format!("{vis}/")) {
                0
            } else if search_region_country(&h.region_id) == search_region_country(vis)
                && !search_region_country(vis).is_empty()
            {
                1
            } else {
                2
            };
            let dist = match (lat, lon) {
                (Some(lat), Some(lon)) => search_haversine_km(lat, lon, h.lat, h.lon),
                _ => 0,
            };
            (region_c, dist)
        }
        None => (1, 0),
    };
    (match_bucket, kind_c, region_c, dist, h.name.len())
}

/// Local FTS5 name index for settlements, POIs, huts, peaks, and named ways.
pub struct NameIndex {
    conn: Connection,
}

impl NameIndex {
    pub fn open_in_memory() -> SqlResult<Self> {
        let conn = Connection::open_in_memory()?;
        Self::migrate(&conn)?;
        Self::migrate_region_osm_pk(&conn).map_err(|e| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(e.to_string())))
        })?;
        localized_store::ensure_names_json_column(&conn)?;
        Self::backfill_legacy_complete(&conn)?;
        Ok(Self { conn })
    }

    fn wal_path(db: &Path) -> PathBuf {
        PathBuf::from(format!("{}-wal", db.display()))
    }

    fn shm_path(db: &Path) -> PathBuf {
        PathBuf::from(format!("{}-shm", db.display()))
    }

    /// Drop a stale `-shm` and checkpoint `-wal` into the main file.
    ///
    /// A crash mid-index can leave a large WAL + torn shared-memory index;
    /// Android's SQLite then SIGBUS in `walIndexReadHdr` on the next readonly
    /// open (Java cannot catch that). Call before any open, including probes.
    /// Does not bump schema version or rebuild FTS.
    pub fn recover_wal_sidecars(path: impl AsRef<Path>) -> Result<(), String> {
        let path = path.as_ref();
        if !path.is_file() {
            return Ok(());
        }
        let shm = Self::shm_path(path);
        let wal = Self::wal_path(path);
        // Stale shm after process death is the usual SIGBUS trigger; safe to
        // remove when no live connection holds the DB (caller must force-stop
        // writers first, or hold the build lock).
        if shm.is_file() {
            let _ = std::fs::remove_file(&shm);
        }
        if !wal.is_file() {
            return Ok(());
        }
        let conn = Connection::open(path).map_err(|e| format!("recover open: {e}"))?;
        conn.busy_timeout(Duration::from_secs(120))
            .map_err(|e| format!("recover busy_timeout: {e}"))?;
        // Avoid mmap of torn pages during checkpoint.
        let _ = conn.execute_batch("PRAGMA mmap_size=0;");
        match conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?))
        }) {
            Ok((busy, log, ckpt)) => {
                log::info!(
                    target: "PlaceIndex",
                    "place_index_wal_recover path={} busy={busy} log={log} checkpointed={ckpt}",
                    path.display()
                );
                Ok(())
            }
            Err(e) => Err(format!("wal_checkpoint: {e}")),
        }
    }

    /// Checkpoint WAL into the main DB (best-effort). Call after a successful
    /// region build so a force-stop cannot leave a 100MB+ sidecar.
    pub fn checkpoint_wal_truncate(&self) -> SqlResult<()> {
        let _ = self.conn.execute_batch("PRAGMA mmap_size=0;");
        self.conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        Ok(())
    }

    pub fn open(path: impl AsRef<Path>) -> SqlResult<Self> {
        let path = path.as_ref();
        // Must run before the readonly user_version peek — that peek SIGBUS'd
        // on tablet after a torn Tunisia WAL.
        let _ = Self::recover_wal_sidecars(path);
        let user_peek: i32 =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .ok()
                .and_then(|c| {
                    c.query_row("PRAGMA user_version", [], |row| row.get(0))
                        .ok()
                })
                .unwrap_or(0);
        if user_peek > 0 && user_peek < PLACE_INDEX_SCHEMA_VERSION && path.is_file() {
            let bak = path.with_file_name("place_index.db.bak-pk-v6");
            if !bak.is_file() {
                std::fs::copy(path, &bak)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            }
        }
        let conn = Connection::open(path)?;
        // WAL writers rarely wait on readers; keep a short timeout for checkpoints.
        // PK migrate copies millions of rows — allow a long busy wait so a stray
        // readonly probe cannot abort the writer with SQLITE_BUSY / locking protocol.
        conn.busy_timeout(Duration::from_secs(120))?;
        let _ = conn.execute_batch("PRAGMA mmap_size=0;");
        let pragmas_t0 = phase_timing::start("place_index.open_db.pragmas");
        Self::apply_file_pragmas(&conn)?;
        phase_timing::end("place_index.open_db.pragmas", pragmas_t0);
        let user_version: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap_or(0);
        // Already at current schema: no DDL, no FTS drop/rebuild.
        // Additive columns (names_json) still apply when version is current —
        // must not bump PLACE_INDEX_SCHEMA_VERSION (that would fail intact checks).
        // Do not run ALTER before migrate on a brand-new DB (no name_entries yet).
        if user_version >= PLACE_INDEX_SCHEMA_VERSION {
            localized_store::ensure_names_json_column(&conn)?;
            return Ok(Self { conn });
        }
        // GROUP BY over name_entries on every open dominated reopen (~383 ms of
        // 384 ms for 2.1M rows in the ignored timing test; tablet open_db ~7 s).
        // Production writers (`load_from_pbf_for_region` → `upsert_build_progress`)
        // always upsert name_index_build. `upsert_entry*` is tests-only and does
        // not; those DBs already have the table from `open`, so they skip too.
        let had_build_table = Self::name_index_build_table_exists(&conn)?;
        let migrate_t0 = phase_timing::start("place_index.open_db.migrate");
        Self::migrate(&conn)?;
        Self::migrate_region_osm_pk(&conn).map_err(|e| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(e.to_string())))
        })?;
        // PK migrate recreates name_entries — re-apply additive columns.
        localized_store::ensure_names_json_column(&conn)?;
        phase_timing::end("place_index.open_db.migrate", migrate_t0);
        let backfill_t0 = phase_timing::start("place_index.open_db.backfill_legacy_complete");
        if !had_build_table {
            Self::backfill_legacy_complete(&conn)?;
        }
        phase_timing::end("place_index.open_db.backfill_legacy_complete", backfill_t0);
        Ok(Self { conn })
    }

    pub fn pragma_quick_check(&self) -> String {
        self.conn
            .query_row("PRAGMA quick_check", [], |row| row.get(0))
            .unwrap_or_else(|e| e.to_string())
    }

    /// Query-only open: no schema-version migrate / FTS rebuild, no long busy wait.
    ///
    /// Still applies additive `names_json` once via a short read-write open when
    /// missing — search SELECTs that column, and pre-i18n intact indexes otherwise
    /// fail with `no such column: names_json` on every query.
    pub fn open_readonly(path: impl AsRef<Path>) -> SqlResult<Self> {
        let path = path.as_ref();
        let _ = Self::recover_wal_sidecars(path);
        if path.is_file() {
            if let Ok(w) = Connection::open(path) {
                let _ = w.busy_timeout(Duration::from_millis(500));
                let _ = w.execute_batch("PRAGMA mmap_size=0;");
                let _ = localized_store::ensure_names_json_column(&w);
            }
        }
        let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        conn.busy_timeout(Duration::from_millis(250))?;
        let _ = conn.execute_batch("PRAGMA mmap_size=0;");
        Ok(Self { conn })
    }

    fn apply_file_pragmas(conn: &Connection) -> SqlResult<()> {
        // WAL lets overlay/search readers see a consistent snapshot during writes.
        // NORMAL (not FULL) is the SQLite-recommended pairing with WAL: fsync on
        // WAL frame commit is skipped; a checkpoint still durable enough for a
        // rebuildable search index.
        let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
        if !mode.eq_ignore_ascii_case("wal") {
            log::warn!(
                target: "NaviSearch",
                "place-index: journal_mode={mode}, expected wal"
            );
        }
        conn.execute_batch("PRAGMA synchronous = NORMAL;")?;
        Ok(())
    }

    fn migrate(conn: &Connection) -> SqlResult<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS name_entries (
                osm_id INTEGER PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                lat REAL NOT NULL,
                lon REAL NOT NULL,
                sub_area TEXT NOT NULL DEFAULT '',
                municipality TEXT NOT NULL DEFAULT '',
                region_id TEXT NOT NULL DEFAULT '',
                search_doc TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS name_index_build (
                region_id TEXT PRIMARY KEY NOT NULL,
                expected INTEGER NOT NULL DEFAULT 0,
                written INTEGER NOT NULL DEFAULT 0,
                complete INTEGER NOT NULL DEFAULT 0,
                source_sha256 TEXT NOT NULL DEFAULT '',
                index_source TEXT NOT NULL DEFAULT ''
            );
            ",
        )?;
        Self::ensure_context_columns(conn)?;
        // Never rebuild FTS here: `INSERT INTO name_fts(name_fts) VALUES('rebuild')`
        // on a multi-million-row tablet DB is a single uninterruptible statement
        // (the 13-minute Oslo block). Batched fill lives in migrate_region_osm_pk.
        Self::ensure_source_sha256_column(conn)?;
        Self::ensure_index_source_column(conn)?;
        localized_store::ensure_names_json_column(conn)?;
        Ok(())
    }

    fn ensure_source_sha256_column(conn: &Connection) -> SqlResult<()> {
        let mut has = false;
        let mut stmt = conn.prepare("PRAGMA table_info(name_index_build)")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
        for r in rows {
            if r?.as_str() == "source_sha256" {
                has = true;
                break;
            }
        }
        if !has {
            conn.execute(
                "ALTER TABLE name_index_build ADD COLUMN source_sha256 TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        Ok(())
    }

    fn ensure_index_source_column(conn: &Connection) -> SqlResult<()> {
        let mut has = false;
        let mut stmt = conn.prepare("PRAGMA table_info(name_index_build)")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
        for r in rows {
            if r?.as_str() == "index_source" {
                has = true;
                break;
            }
        }
        if !has {
            conn.execute(
                "ALTER TABLE name_index_build ADD COLUMN index_source TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        Ok(())
    }

    fn region_row_count(conn: &Connection, region_id: &str) -> SqlResult<i64> {
        conn.query_row(
            "SELECT COUNT(*) FROM name_entries WHERE region_id = ?1",
            params![region_id],
            |row| row.get(0),
        )
    }

    /// Per-region row counts already present (no hard-coded product/test region ids).
    fn region_row_counts(conn: &Connection) -> SqlResult<Vec<(String, i64)>> {
        let mut stmt = conn.prepare(
            "SELECT region_id, COUNT(*) FROM name_entries GROUP BY region_id ORDER BY region_id",
        )?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect()
    }

    fn name_entries_sql(conn: &Connection) -> SqlResult<Option<String>> {
        conn.query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='name_entries'",
            [],
            |row| row.get(0),
        )
        .optional()
    }

    /// v6: PRIMARY KEY (region_id, osm_id). Resumable copy into name_entries_pk
    /// in small transactions so a plan can pause within 2 s.
    fn migrate_region_osm_pk(conn: &Connection) -> anyhow::Result<()> {
        let sql = Self::name_entries_sql(conn)?.unwrap_or_default();
        if sql.contains("PRIMARY KEY (region_id, osm_id)")
            || sql.contains("PRIMARY KEY(region_id, osm_id)")
        {
            Self::ensure_search_doc_fts_create_only(conn)?;
            Self::ensure_search_doc_fts_batched(conn)?;
            conn.execute_batch(&format!(
                "PRAGMA user_version = {PLACE_INDEX_SCHEMA_VERSION};"
            ))?;
            return Ok(());
        }
        index_phase_tick(IndexBuildPhase::Migrate);
        pause_index_if_plan_active(0, 0, place_index_rss_mb())?;
        let before = Self::region_row_counts(conn)?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS name_entries_pk (
                region_id TEXT NOT NULL DEFAULT '',
                osm_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                lat REAL NOT NULL,
                lon REAL NOT NULL,
                sub_area TEXT NOT NULL DEFAULT '',
                municipality TEXT NOT NULL DEFAULT '',
                search_doc TEXT NOT NULL DEFAULT '',
                PRIMARY KEY (region_id, osm_id)
            );
            CREATE TABLE IF NOT EXISTS _pk_migrate_pos (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                last_rowid INTEGER NOT NULL
            );
            INSERT OR IGNORE INTO _pk_migrate_pos(id, last_rowid) VALUES (1, -1);
            ",
        )?;
        let source_n: i64 =
            conn.query_row("SELECT COUNT(*) FROM name_entries", [], |row| row.get(0))?;
        loop {
            index_phase_tick(IndexBuildPhase::Migrate);
            let last_rowid: i64 = conn
                .query_row(
                    "SELECT last_rowid FROM _pk_migrate_pos WHERE id = 1",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(-1);
            let copied_so_far: i64 = conn
                .query_row("SELECT COUNT(*) FROM name_entries_pk", [], |r| r.get(0))
                .unwrap_or(0);
            pause_index_if_plan_active(copied_so_far, source_n, place_index_rss_mb())?;
            let end_rowid: Option<i64> = conn.query_row(
                "SELECT MAX(rowid) FROM (
                    SELECT rowid FROM name_entries WHERE rowid > ?1
                    ORDER BY rowid LIMIT ?2
                 )",
                params![last_rowid, MIGRATE_COPY_BATCH],
                |row| row.get(0),
            )?;
            let Some(end_rowid) = end_rowid else {
                break;
            };
            conn.execute(
                "
                INSERT OR IGNORE INTO name_entries_pk(
                    region_id, osm_id, name, kind, lat, lon, sub_area, municipality, search_doc
                )
                SELECT e.region_id, e.osm_id, e.name, e.kind, e.lat, e.lon,
                       e.sub_area, e.municipality,
                       COALESCE(NULLIF(e.search_doc, ''), e.name)
                FROM name_entries e
                WHERE e.rowid > ?1 AND e.rowid <= ?2
                ",
                params![last_rowid, end_rowid],
            )?;
            conn.execute(
                "UPDATE _pk_migrate_pos SET last_rowid = ?1 WHERE id = 1",
                params![end_rowid],
            )?;
        }
        index_phase_tick(IndexBuildPhase::Commit);
        pause_index_if_plan_active(source_n, source_n, place_index_rss_mb())?;
        conn.execute_batch("DROP TABLE IF EXISTS name_fts;")?;
        loop {
            index_phase_tick(IndexBuildPhase::Commit);
            pause_index_if_plan_active(source_n, source_n, place_index_rss_mb())?;
            let deleted = conn.execute(
                "DELETE FROM name_entries WHERE rowid IN (
                    SELECT rowid FROM name_entries LIMIT ?1
                )",
                params![MIGRATE_COPY_BATCH],
            )?;
            if deleted == 0 {
                break;
            }
        }
        index_phase_tick(IndexBuildPhase::Commit);
        pause_index_if_plan_active(source_n, source_n, place_index_rss_mb())?;
        conn.execute_batch(
            "
            DROP TABLE name_entries;
            ALTER TABLE name_entries_pk RENAME TO name_entries;
            DROP TABLE IF EXISTS _pk_migrate_pos;
            ",
        )?;
        Self::ensure_search_doc_fts_create_only(conn)?;
        Self::ensure_search_doc_fts_batched(conn)?;
        for (rid, n) in &before {
            let after = Self::region_row_count(conn, rid).unwrap_or(0);
            if after != *n {
                anyhow::bail!("place_index pk migrate stopped: {rid} rows {after} != {n}");
            }
        }
        conn.execute_batch(&format!(
            "PRAGMA user_version = {PLACE_INDEX_SCHEMA_VERSION};"
        ))?;
        Ok(())
    }

    fn ensure_search_doc_fts_batched(conn: &Connection) -> anyhow::Result<()> {
        let total: i64 =
            conn.query_row("SELECT COUNT(*) FROM name_entries", [], |row| row.get(0))?;
        let mut last_rowid: i64 = -1;
        loop {
            index_phase_tick(IndexBuildPhase::Fts);
            pause_index_if_plan_active(last_rowid, total, place_index_rss_mb())?;
            let end_rowid: Option<i64> = conn.query_row(
                "SELECT MAX(rowid) FROM (
                    SELECT rowid FROM name_entries WHERE rowid > ?1
                    ORDER BY rowid LIMIT ?2
                 )",
                params![last_rowid, MIGRATE_COPY_BATCH],
                |row| row.get(0),
            )?;
            let Some(end_rowid) = end_rowid else {
                break;
            };
            conn.execute(
                "
                INSERT INTO name_fts(rowid, search_doc, kind)
                SELECT rowid, search_doc, kind FROM name_entries
                WHERE rowid > ?1 AND rowid <= ?2
                ",
                params![last_rowid, end_rowid],
            )?;
            last_rowid = end_rowid;
        }
        Ok(())
    }

    fn name_index_build_table_exists(conn: &Connection) -> SqlResult<bool> {
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'name_index_build'",
            [],
            |row| row.get(0),
        )?;
        Ok(n > 0)
    }

    /// Existing DBs have rows but no build-progress row; treat them as finished
    /// so cache_hit still skips a rebuild. Never overwrite an in-progress row.
    fn backfill_legacy_complete(conn: &Connection) -> SqlResult<()> {
        conn.execute_batch(
            "
            INSERT OR IGNORE INTO name_index_build(region_id, expected, written, complete)
            SELECT region_id, COUNT(*), COUNT(*), 1
            FROM name_entries
            WHERE region_id != ''
            GROUP BY region_id;
            INSERT OR IGNORE INTO name_index_build(region_id, expected, written, complete)
            SELECT '', COUNT(*), COUNT(*), 1
            FROM name_entries
            WHERE region_id = ''
            HAVING COUNT(*) > 0;
            ",
        )?;
        Ok(())
    }

    fn ensure_context_columns(conn: &Connection) -> SqlResult<()> {
        let mut has_sub = false;
        let mut has_muni = false;
        let mut has_region = false;
        let mut has_search_doc = false;
        let mut stmt = conn.prepare("PRAGMA table_info(name_entries)")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
        for r in rows {
            match r?.as_str() {
                "sub_area" => has_sub = true,
                "municipality" => has_muni = true,
                "region_id" => has_region = true,
                "search_doc" => has_search_doc = true,
                _ => {}
            }
        }
        if !has_sub {
            conn.execute(
                "ALTER TABLE name_entries ADD COLUMN sub_area TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        if !has_muni {
            conn.execute(
                "ALTER TABLE name_entries ADD COLUMN municipality TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        if !has_region {
            conn.execute(
                "ALTER TABLE name_entries ADD COLUMN region_id TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        if !has_search_doc {
            conn.execute(
                "ALTER TABLE name_entries ADD COLUMN search_doc TEXT NOT NULL DEFAULT ''",
                [],
            )?;
            // Legacy rows: FTS rebuild can use display name until the region is
            // re-indexed with alt_name / loc_name. Batched in migrate_region_osm_pk
            // so a plan can pause; a single UPDATE of 11M rows cannot.
        }
        // After v6 the composite PK (region_id, osm_id) covers region lookups.
        // Creating idx_name_entries_region_id on a stale multi-million-row file
        // is another uninterruptible stall.
        Ok(())
    }

    /// FTS5 external-content table must index `search_doc` (v5). Recreate when
    /// an older `name`/`kind` FTS definition is still present.
    #[allow(dead_code)]
    fn ensure_search_doc_fts(conn: &Connection) -> SqlResult<()> {
        let sql: Option<String> = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'name_fts'",
                [],
                |row| row.get(0),
            )
            .ok();
        let needs_recreate = match sql.as_deref() {
            None => true,
            Some(s) => !s.contains("search_doc") || !s.contains("content_rowid='rowid'"),
        };
        if needs_recreate {
            conn.execute_batch("DROP TABLE IF EXISTS name_fts;")?;
            conn.execute_batch(
                "
                CREATE VIRTUAL TABLE name_fts USING fts5(
                    search_doc,
                    kind,
                    content='name_entries',
                    content_rowid='rowid'
                );
                ",
            )?;
            let _ = conn.execute_batch("INSERT INTO name_fts(name_fts) VALUES('rebuild');");
        }
        Ok(())
    }

    fn ensure_search_doc_fts_create_only(conn: &Connection) -> SqlResult<()> {
        conn.execute_batch("DROP TABLE IF EXISTS name_fts;")?;
        conn.execute_batch(
            "
            CREATE VIRTUAL TABLE name_fts USING fts5(
                search_doc,
                kind,
                content='name_entries',
                content_rowid='rowid'
            );
            ",
        )?;
        Ok(())
    }

    /// True when `name_entries` has at least one row (built index, not a stub).
    /// Read-only: never creates the DB file.
    pub fn has_entries(path: impl AsRef<Path>) -> bool {
        let path = path.as_ref();
        if !path.is_file() {
            return false;
        }
        let Ok(conn) =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            return false;
        };
        conn.query_row("SELECT 1 FROM name_entries LIMIT 1", [], |_| Ok(()))
            .is_ok()
    }

    /// True when this DB already has at least one row for `region_id`.
    pub fn has_entries_for_region(path: impl AsRef<Path>, region_id: &str) -> bool {
        let path = path.as_ref();
        let region_id = region_id.trim().trim_matches('/');
        if !path.is_file() || region_id.is_empty() {
            return false;
        }
        let _ = Self::recover_wal_sidecars(path);
        let Ok(conn) =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            return false;
        };
        conn.query_row(
            "SELECT 1 FROM name_entries WHERE region_id = ?1 LIMIT 1",
            params![region_id],
            |_| Ok(()),
        )
        .is_ok()
    }

    /// True when this DB was built by a context-aware `load_from_pbf`.
    /// Read-only: never creates or migrates the file.
    pub fn is_current_schema(path: impl AsRef<Path>) -> bool {
        let path = path.as_ref();
        let _ = Self::recover_wal_sidecars(path);
        let Ok(conn) =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            return false;
        };
        let v: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap_or(0);
        v >= PLACE_INDEX_SCHEMA_VERSION
    }

    /// Schema bumps must migrate in place. Never delete the shared multi-region DB.
    ///
    /// Returns false always (no file removed). Callers that cannot migrate must
    /// stop and ask — they must not wipe sibling regions.
    pub fn discard_if_schema_stale(path: impl AsRef<Path>) -> bool {
        let path = path.as_ref();
        if !path.is_file() || Self::is_current_schema(path) {
            return false;
        }
        log::warn!(
            target: "NaviSearch",
            "place-index: schema older than v{PLACE_INDEX_SCHEMA_VERSION} at {} — \
             migrate in place, not delete",
            path.display()
        );
        false
    }

    /// Path of the one-shot v5→v6 pre-migrate copy next to `place_index.db`.
    pub fn bak_pk_v6_path(path: impl AsRef<Path>) -> PathBuf {
        path.as_ref().with_file_name("place_index.db.bak-pk-v6")
    }

    /// Restore `place_index.db` from [`bak_pk_v6_path`] when the live file is
    /// missing, tiny, or was replaced by an empty post-quarantine stub.
    ///
    /// Returns true when a restore ran. Never deletes the bak file.
    pub fn restore_from_bak_pk_v6(path: impl AsRef<Path>) -> bool {
        let path = path.as_ref();
        let bak = Self::bak_pk_v6_path(path);
        if !bak.is_file() {
            return false;
        }
        let bak_len = std::fs::metadata(&bak).map(|m| m.len()).unwrap_or(0);
        if bak_len < 10_000 {
            return false;
        }
        let live_len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let need = !path.is_file() || live_len < 10_000 || live_len + 1_000_000 < bak_len;
        if !need {
            return false;
        }
        for suffix in ["-wal", "-shm"] {
            let mut s = path.as_os_str().to_os_string();
            s.push(suffix);
            let _ = std::fs::remove_file(PathBuf::from(s));
        }
        match std::fs::copy(&bak, path) {
            Ok(_) => {
                log::warn!(
                    target: "NaviSearch",
                    "place-index: restored {} ({} bytes) from {}",
                    path.display(),
                    bak_len,
                    bak.display()
                );
                true
            }
            Err(e) => {
                log::error!(
                    target: "NaviSearch",
                    "place-index: restore from {} failed: {e}",
                    bak.display()
                );
                false
            }
        }
    }

    /// Intact check on an already-open writer (no second connection).
    pub fn region_intact_on_conn(&self, region_id: &str) -> bool {
        let region_id = region_id.trim().trim_matches('/');
        if region_id.is_empty() {
            return false;
        }
        let user_version: i32 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap_or(0);
        if user_version < PLACE_INDEX_SCHEMA_VERSION {
            return false;
        }
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM name_entries WHERE region_id = ?1",
                params![region_id],
                |row| row.get(0),
            )
            .unwrap_or(0);
        if count <= 0 {
            return false;
        }
        match self.conn.query_row(
            "SELECT complete, written, expected FROM name_index_build WHERE region_id = ?1",
            params![region_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        ) {
            Ok((complete, written, expected)) => {
                if complete == 0 {
                    return false;
                }
                let refer = if written > 0 { written } else { expected };
                refer <= 0 || (count as f64) >= (refer as f64) * 0.10
            }
            Err(_) => true,
        }
    }

    /// Rename a DB that cannot be opened so the next open sees "missing" without
    /// destroying the bytes. WAL/SHM sidecars stay next to the quarantine name.
    pub fn quarantine_unreadable(path: impl AsRef<Path>) -> Option<PathBuf> {
        let path = path.as_ref();
        if !path.is_file() {
            return None;
        }
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let file_name = path.file_name()?.to_string_lossy();
        let dest = path.with_file_name(format!("{file_name}.quarantine-{ts}"));
        match std::fs::rename(path, &dest) {
            Ok(()) => {
                for suffix in ["-wal", "-shm"] {
                    let src = {
                        let mut s = path.as_os_str().to_os_string();
                        s.push(suffix);
                        PathBuf::from(s)
                    };
                    if src.is_file() {
                        let mut d = dest.as_os_str().to_os_string();
                        d.push(suffix);
                        let _ = std::fs::rename(&src, PathBuf::from(d));
                    }
                }
                log::error!(
                    target: "NaviSearch",
                    "place-index: quarantined unreadable DB {} -> {}",
                    path.display(),
                    dest.display()
                );
                Some(dest)
            }
            Err(e) => {
                log::error!(
                    target: "NaviSearch",
                    "place-index: quarantine failed for {}: {e}",
                    path.display()
                );
                None
            }
        }
    }

    #[allow(dead_code)]
    fn remove_wal_sidecars(path: &Path) {
        for suffix in ["-wal", "-shm"] {
            let mut sidecar = path.as_os_str().to_os_string();
            sidecar.push(suffix);
            let _ = std::fs::remove_file(PathBuf::from(sidecar));
        }
    }

    /// True when this region's index finished a write **and** has a plausible
    /// row count (not `complete=1` with 0 or a handful of rows vs a million written).
    ///
    /// Missing `name_index_build` with real rows is legacy intact. Complete with
    /// zero rows is **not** intact.
    pub fn region_index_intact(path: impl AsRef<Path>, region_id: &str) -> bool {
        let path = path.as_ref();
        let region_id = region_id.trim().trim_matches('/');
        if !path.is_file() {
            return false;
        }
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.len() < 10_000 {
                return false;
            }
        }
        if !Self::is_current_schema(path) {
            return false;
        }
        let Ok(conn) =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            return false;
        };
        let count: i64 = if region_id.is_empty() {
            conn.query_row("SELECT COUNT(*) FROM name_entries", [], |row| row.get(0))
                .unwrap_or(0)
        } else {
            conn.query_row(
                "SELECT COUNT(*) FROM name_entries WHERE region_id = ?1",
                params![region_id],
                |row| row.get(0),
            )
            .unwrap_or(0)
        };
        if count <= 0 {
            return false;
        }
        match conn.query_row(
            "SELECT complete, written, expected FROM name_index_build WHERE region_id = ?1",
            params![region_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        ) {
            Ok((complete, written, expected)) => {
                if complete == 0 {
                    return false;
                }
                let refer = if written > 0 { written } else { expected };
                refer <= 0 || (count as f64) >= (refer as f64) * 0.10
            }
            Err(_) => true, // legacy: rows exist, no build row / table
        }
    }

    /// True when this region's index finished a write (not a mid-build partial).
    ///
    /// Missing `name_index_build` table (never opened with this code) falls back
    /// to [`has_entries_for_region`] / [`has_entries`] so legacy DBs still cache-hit.
    /// Prefers [`region_index_intact`] so empty complete flags are not treated as ready.
    pub fn region_index_complete(path: impl AsRef<Path>, region_id: &str) -> bool {
        Self::region_index_intact(path, region_id)
    }

    fn build_is_interrupted(conn: &Connection, region_id: &str) -> bool {
        conn.query_row(
            "SELECT complete FROM name_index_build WHERE region_id = ?1",
            params![region_id],
            |row| row.get::<_, i64>(0),
        )
        .ok()
        .is_some_and(|v| v == 0)
    }

    fn build_written(conn: &Connection, region_id: &str) -> i64 {
        conn.query_row(
            "SELECT written FROM name_index_build WHERE region_id = ?1",
            params![region_id],
            |row| row.get(0),
        )
        .unwrap_or(0)
    }

    /// Full-file rebuild with empty `region_id` (legacy / single-extract callers).
    pub fn load_from_pbf(&mut self, path: impl AsRef<Path>) -> anyhow::Result<usize> {
        self.load_from_pbf_for_region(path, "")
    }

    /// Index one region's PBF into the shared DB without wiping other regions.
    ///
    /// When `region_id` is non-empty, only that region's rows are replaced.
    /// When empty, the whole table is cleared (legacy single-extract path).
    pub fn load_from_pbf_for_region(
        &mut self,
        path: impl AsRef<Path>,
        region_id: &str,
    ) -> anyhow::Result<usize> {
        let total_t0 = phase_timing::start("place_index.total");
        let _bg = crate::download::pbf_priority::BackgroundIndexerGuard::enter();
        // Same thread already holds the plan: refuse the build immediately.
        // Do not wait for that plan (deadlock) and do not trip `NameIndex::open`.
        if crate::download::pbf_priority::plan_on_this_thread() {
            anyhow::bail!("{PLACE_INDEX_PAUSED_PREFIX} written=0 expected=0");
        }
        let path = path.as_ref();
        let region_id = region_id.trim().trim_matches('/').to_string();
        let mut peak_rss_mb = place_index_rss_mb();
        let bump_peak = |peak: &mut u64| {
            let rss = place_index_rss_mb();
            if rss > *peak {
                *peak = rss;
            }
        };
        // Before any PBF pass: a plan already in flight must not enter a scan
        // that waits on foreground_plan_active (that parks the indexer).
        index_phase_tick(IndexBuildPhase::ReadSource);
        pause_index_if_plan_active(Self::build_written(&self.conn, &region_id), 0, peak_rss_mb)?;
        // (osm_id, display_name, search_doc, kind, names_json, lat, lon)
        let mut batch: Vec<(i64, String, String, String, String, f64, f64)> = Vec::new();
        const PHASES: u64 = 6;
        let interrupted = Self::build_is_interrupted(&self.conn, &region_id);
        let phase_prefix = if interrupted {
            "Place index: previous build interrupted, restarting — "
        } else {
            "Place index: "
        };
        // Admin polygons use their own PBF passes (relations → ways → nodes).
        // Sub-labels are set inside load_admin_from_pbf so the 0/6 phase is not
        // a single frozen "admin boundaries…" string for a minute-plus scan.
        let admin_t0 = phase_timing::start("place_index.admin");
        let admin_rings = match place_context::load_admin_from_pbf(path, phase_prefix) {
            Ok(v) => v,
            Err(e) if e.to_string().contains(PLACE_INDEX_PAUSED_PREFIX) => {
                return Err(e);
            }
            Err(e) => {
                log::warn!("admin boundary load for place context skipped: {e:#}");
                Vec::new()
            }
        };
        phase_timing::end_detail(
            "place_index.admin",
            admin_t0,
            &format!("admin_rings={}", admin_rings.len()),
        );
        bump_peak(&mut peak_rss_mb);
        pause_index_if_plan_active(0, 0, peak_rss_mb)?;

        // Pass 1: collect named closed/open ways that need node centroids
        // (tourism=zoo, amenity areas, etc. are often ways, not nodes).
        crate::download::progress::set(1, Some(PHASES), &format!("{phase_prefix}scanning ways…"));
        let ways_t0 = phase_timing::start("place_index.ways");
        let mut way_jobs: Vec<(i64, String, String, String, String, Vec<i64>)> = Vec::new();
        let mut needed_nodes: std::collections::HashSet<i64> = std::collections::HashSet::new();
        let mut ways_visited = 0usize;
        let mut ways_last_hb = 0usize;
        let ways_pause = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        {
            let ways_pause = ways_pause.clone();
            crate::download::pbf_priority::for_each_pbf_elements_pausable(path, |element| {
                if ways_pause.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                ways_visited += 1;
                if ways_visited - ways_last_hb >= PLACE_INDEX_PROGRESS_HEARTBEAT {
                    ways_last_hb = ways_visited;
                    index_phase_tick(IndexBuildPhase::ReadSource);
                    crate::download::progress::set(
                        1,
                        Some(PHASES),
                        &format!("{phase_prefix}scanning ways… ({} found)", way_jobs.len()),
                    );
                    if pause_index_if_plan_active(0, 0, place_index_rss_mb()).is_err() {
                        ways_pause.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                }
                let Element::Way(way) = element else {
                    return;
                };
                let tags: Vec<(String, String)> = way
                    .tags()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                let Some((_, name, search_doc, kind, names_json, _, _)) = classify_named(
                    way.id(),
                    0.0,
                    0.0,
                    tags.iter().map(|(k, v)| (k.as_str(), v.as_str())),
                ) else {
                    return;
                };
                // Skip unnamed highway geometries — those flood the index; keep
                // amenity/tourism/place/leisure/shop and other classified POIs.
                if kind.starts_with("highway:") {
                    return;
                }
                let refs: Vec<i64> = way.refs().collect();
                if refs.is_empty() {
                    return;
                }
                for id in &refs {
                    needed_nodes.insert(*id);
                }
                way_jobs.push((way.id(), name, search_doc, kind, names_json, refs));
            })?;
        }
        if ways_pause.load(std::sync::atomic::Ordering::Relaxed) {
            pause_index_if_plan_active(0, 0, peak_rss_mb)?;
        }
        phase_timing::end_detail(
            "place_index.ways",
            ways_t0,
            &format!(
                "way_jobs={} needed_nodes={}",
                way_jobs.len(),
                needed_nodes.len()
            ),
        );
        bump_peak(&mut peak_rss_mb);
        pause_index_if_plan_active(0, 0, peak_rss_mb)?;

        // Pass 2: nodes (search hits) + coords for way centroids.
        crate::download::progress::set(2, Some(PHASES), &format!("{phase_prefix}scanning nodes…"));
        let nodes_t0 = phase_timing::start("place_index.nodes");
        let mut node_coords: std::collections::HashMap<i64, (f64, f64)> =
            std::collections::HashMap::with_capacity(needed_nodes.len());
        let mut nodes_visited = 0usize;
        let mut nodes_last_hb = 0usize;
        let nodes_pause = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        {
            let nodes_pause = nodes_pause.clone();
            crate::download::pbf_priority::for_each_pbf_elements_pausable(path, |element| {
                if nodes_pause.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                nodes_visited += 1;
                if nodes_visited - nodes_last_hb >= PLACE_INDEX_PROGRESS_HEARTBEAT {
                    nodes_last_hb = nodes_visited;
                    index_phase_tick(IndexBuildPhase::ReadSource);
                    crate::download::progress::set(
                        2,
                        Some(PHASES),
                        &format!("{phase_prefix}scanning nodes… ({} found)", batch.len()),
                    );
                    if pause_index_if_plan_active(0, 0, place_index_rss_mb()).is_err() {
                        nodes_pause.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                }
                match element {
                    Element::Node(node) => {
                        let id = node.id();
                        let lat = node.lat();
                        let lon = node.lon();
                        if needed_nodes.contains(&id) {
                            node_coords.insert(id, (lat, lon));
                        }
                        if let Some(hit) = classify_named(id, lat, lon, node.tags()) {
                            batch.push(hit);
                        }
                    }
                    Element::DenseNode(node) => {
                        let id = node.id;
                        let lat = node.lat();
                        let lon = node.lon();
                        if needed_nodes.contains(&id) {
                            node_coords.insert(id, (lat, lon));
                        }
                        if let Some(hit) = classify_named(id, lat, lon, node.tags()) {
                            batch.push(hit);
                        }
                    }
                    _ => {}
                }
            })?;
        }
        if nodes_pause.load(std::sync::atomic::Ordering::Relaxed) {
            pause_index_if_plan_active(0, 0, peak_rss_mb)?;
        }
        let node_hits = batch.len();
        phase_timing::end_detail(
            "place_index.nodes",
            nodes_t0,
            &format!(
                "node_hits={} centroid_coords={}",
                node_hits,
                node_coords.len()
            ),
        );
        bump_peak(&mut peak_rss_mb);
        pause_index_if_plan_active(0, 0, peak_rss_mb)?;

        // Surface centroids on the progress UI — this pass used to leave the
        // last "scanning nodes…" label frozen for a long stretch.
        crate::download::progress::set(2, Some(PHASES), &format!("{phase_prefix}way centroids…"));
        let centroids_t0 = phase_timing::start("place_index.way_centroids");
        let mut way_hits = 0usize;
        let centroid_total = way_jobs.len();
        for (i, (way_id, name, search_doc, kind, names_json, refs)) in way_jobs.into_iter().enumerate() {
            let mut sum_lat = 0.0;
            let mut sum_lon = 0.0;
            let mut n = 0usize;
            for id in refs {
                if let Some((lat, lon)) = node_coords.get(&id) {
                    sum_lat += lat;
                    sum_lon += lon;
                    n += 1;
                }
            }
            if n == 0 {
                continue;
            }
            batch.push((
                way_id,
                name,
                search_doc,
                kind,
                names_json,
                sum_lat / n as f64,
                sum_lon / n as f64,
            ));
            way_hits += 1;
            if (i + 1) % PLACE_INDEX_PROGRESS_HEARTBEAT == 0 {
                crate::download::progress::set(
                    2,
                    Some(PHASES),
                    &format!("{phase_prefix}way centroids… ({way_hits} / {centroid_total})"),
                );
            }
        }
        drop(node_coords);
        phase_timing::end_detail(
            "place_index.way_centroids",
            centroids_t0,
            &format!("way_hits={way_hits} batch={}", batch.len()),
        );
        bump_peak(&mut peak_rss_mb);
        index_phase_tick(IndexBuildPhase::SortPrepare);
        pause_index_if_plan_active(0, batch.len() as i64, peak_rss_mb)?;

        // Official hiking/cycling route relations (name/ref/operator) for To/Via search.
        // Relation ids are distinct from node ids in OSM; store relation id as-is
        // (FTS rowid = osm_id).
        crate::download::progress::set(3, Some(PHASES), &format!("{phase_prefix}named routes…"));
        let routes_t0 = phase_timing::start("place_index.named_routes");
        pause_index_if_plan_active(0, batch.len() as i64, peak_rss_mb)?;
        let mut route_hits = 0usize;
        match crate::routing::graph::load_named_route_entries(path) {
            Ok(routes) => {
                route_hits = routes.len();
                for r in routes {
                    let search_doc = search_doc_with_fold_aliases(&r.name);
                    batch.push((r.osm_id, r.name, search_doc, r.kind, String::new(), r.lat, r.lon));
                }
            }
            Err(e) if e.to_string().contains(PLACE_INDEX_PAUSED_PREFIX) => {
                return Err(e);
            }
            Err(e) => {
                log::warn!("named route relation index skipped: {e:#}");
            }
        }
        phase_timing::end_detail(
            "place_index.named_routes",
            routes_t0,
            &format!("route_hits={route_hits} batch={}", batch.len()),
        );
        bump_peak(&mut peak_rss_mb);
        pause_index_if_plan_active(0, batch.len() as i64, peak_rss_mb)?;

        crate::download::progress::set(
            4,
            Some(PHASES),
            &format!("{phase_prefix}resolving context…"),
        );
        let ctx_t0 = phase_timing::start("place_index.resolve_context");
        let sub_areas = batch
            .iter()
            .filter_map(|(osm_id, name, _search_doc, kind, _names_json, lat, lon)| {
                place_context::sub_area_pt(*osm_id, name.clone(), kind, *lat, *lon)
            })
            .collect();
        let resolver =
            place_context::ContextResolver::from_admin_and_sub_areas(admin_rings, sub_areas);
        phase_timing::end_detail(
            "place_index.resolve_context",
            ctx_t0,
            &format!("batch={}", batch.len()),
        );
        bump_peak(&mut peak_rss_mb);
        pause_index_if_plan_active(0, batch.len() as i64, peak_rss_mb)?;

        crate::download::progress::set(
            5,
            Some(PHASES),
            &format!("{phase_prefix}writing database…"),
        );
        let write_t0 = phase_timing::start("place_index.sqlite_write");
        let resume_written = if interrupted {
            Self::build_written(&self.conn, &region_id)
        } else {
            0
        };
        let resume = interrupted && resume_written > 0;
        let skip: HashSet<i64> = if resume {
            Self::osm_ids_for_region(&self.conn, &region_id)?
        } else {
            HashSet::new()
        };

        let clear_t0 = phase_timing::start("place_index.sqlite_clear_region");
        {
            let tx = self.conn.unchecked_transaction()?;
            if !resume {
                Self::clear_region_rows(&tx, &region_id)?;
            }
            Self::upsert_build_progress(
                &tx,
                &region_id,
                batch.len() as i64,
                if resume { resume_written } else { 0 },
                false,
            )?;
            tx.commit()?;
        }
        phase_timing::end("place_index.sqlite_clear_region", clear_t0);

        let insert_t0 = phase_timing::start("place_index.sqlite_insert_rows");
        let total = batch.len();
        let mut inserted = skip.len();
        let mut since_commit = 0usize;
        index_phase_tick(IndexBuildPhase::Insert);
        pause_index_if_plan_active(inserted as i64, total as i64, peak_rss_mb)?;
        let mut tx = self.conn.unchecked_transaction()?;
        for (osm_id, name, search_doc, kind, names_json, lat, lon) in &batch {
            if skip.contains(osm_id) {
                continue;
            }
            let ctx = resolver.resolve(*osm_id, name, kind, *lat, *lon);
            tx.execute(
                "INSERT OR REPLACE INTO name_entries(osm_id, name, kind, lat, lon, sub_area, municipality, region_id, search_doc, names_json)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    osm_id,
                    name,
                    kind,
                    lat,
                    lon,
                    ctx.sub_area,
                    ctx.municipality,
                    region_id,
                    search_doc,
                    names_json
                ],
            )?;
            let rowid: i64 = tx.query_row(
                "SELECT rowid FROM name_entries WHERE region_id = ?1 AND osm_id = ?2",
                params![region_id, osm_id],
                |row| row.get(0),
            )?;
            let _ = tx.execute(
                "INSERT INTO name_fts(name_fts, rowid, search_doc, kind) VALUES('delete', ?1, NULL, NULL)",
                params![rowid],
            );
            tx.execute(
                "INSERT INTO name_fts(rowid, search_doc, kind) VALUES (?1,?2,?3)",
                params![rowid, search_doc, kind],
            )?;
            inserted += 1;
            since_commit += 1;
            if since_commit >= INSERT_COMMIT_BATCH {
                Self::upsert_build_progress(&tx, &region_id, total as i64, inserted as i64, false)?;
                tx.commit()?;
                crate::download::progress::set(
                    inserted as u64,
                    Some(total as u64),
                    &format!("{phase_prefix}writing database…"),
                );
                bump_peak(&mut peak_rss_mb);
                index_phase_tick(IndexBuildPhase::Insert);
                pause_index_if_plan_active(inserted as i64, total as i64, peak_rss_mb)?;
                tx = self.conn.unchecked_transaction()?;
                since_commit = 0;
            }
        }
        phase_timing::end_detail(
            "place_index.sqlite_insert_rows",
            insert_t0,
            &format!("rows={total} resumed={resume}"),
        );
        let commit_t0 = phase_timing::start("place_index.sqlite_commit");
        tx.execute_batch(&format!(
            "PRAGMA user_version = {PLACE_INDEX_SCHEMA_VERSION};"
        ))?;
        Self::upsert_build_progress(&tx, &region_id, total as i64, inserted as i64, true)?;
        index_phase_tick(IndexBuildPhase::Commit);
        pause_index_if_plan_active(inserted as i64, total as i64, peak_rss_mb)?;
        tx.commit()?;
        phase_timing::end("place_index.sqlite_commit", commit_t0);
        phase_timing::end_detail(
            "place_index.sqlite_write",
            write_t0,
            &format!("rows={total}"),
        );
        crate::download::progress::set(PHASES, Some(PHASES), "Place index ready");
        bump_peak(&mut peak_rss_mb);
        // Fold WAL into the main file so a force-stop right after PASS cannot
        // leave a multi-hundred-MB sidecar that SIGBUS's the next probe.
        if let Err(e) = self.checkpoint_wal_truncate() {
            log::warn!(
                target: "PlaceIndex",
                "place_index_build wal_checkpoint after built region={region_id}: {e}"
            );
        }
        log::info!(
            target: "PlaceIndex",
            "place_index_build action=built region={region_id} indexed={} peak_rss_mb={peak_rss_mb}",
            batch.len()
        );
        phase_timing::end_detail(
            "place_index.total",
            total_t0,
            &format!("indexed={} peak_rss_mb={peak_rss_mb}", batch.len()),
        );
        Ok(batch.len())
    }

    /// Remove all place rows for one region (or the entire index when `region_id` is empty).
    pub fn clear_region(&mut self, region_id: &str) -> SqlResult<()> {
        let region_id = region_id.trim().trim_matches('/');
        let tx = self.conn.unchecked_transaction()?;
        Self::clear_region_rows(&tx, region_id)?;
        tx.commit()?;
        Ok(())
    }

    fn clear_region_rows(tx: &rusqlite::Transaction<'_>, region_id: &str) -> SqlResult<()> {
        if region_id.is_empty() {
            tx.execute_batch(
                "
                DELETE FROM name_entries;
                DELETE FROM name_index_build;
                INSERT INTO name_fts(name_fts) VALUES('rebuild');
                ",
            )?;
            return Ok(());
        }
        {
            let mut stmt = tx.prepare("SELECT rowid FROM name_entries WHERE region_id = ?1")?;
            let ids: Vec<i64> = stmt
                .query_map(params![region_id], |row| row.get(0))?
                .collect::<SqlResult<Vec<_>>>()?;
            drop(stmt);
            for osm_id in ids {
                let _ = tx.execute(
                    "INSERT INTO name_fts(name_fts, rowid, search_doc, kind) VALUES('delete', ?1, NULL, NULL)",
                    params![osm_id],
                );
            }
        }
        tx.execute(
            "DELETE FROM name_entries WHERE region_id = ?1",
            params![region_id],
        )?;
        tx.execute(
            "DELETE FROM name_index_build WHERE region_id = ?1",
            params![region_id],
        )?;
        // External-content FTS5: rebuild after deleting a region's rows.
        let _ = tx.execute_batch("INSERT INTO name_fts(name_fts) VALUES('rebuild');");
        Ok(())
    }

    /// Place-source sha256 recorded with this region's index state, if any.
    pub fn place_source_sha256(path: impl AsRef<Path>, region_id: &str) -> Option<String> {
        let path = path.as_ref();
        let region_id = region_id.trim().trim_matches('/');
        if !path.is_file() || region_id.is_empty() {
            return None;
        }
        let Ok(conn) =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            return None;
        };
        conn.query_row(
            "SELECT source_sha256 FROM name_index_build WHERE region_id = ?1",
            params![region_id],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    }

    /// Record the place-source sha256 on an existing (or just-built) region row.
    /// Does not change complete/written/expected.
    pub fn set_place_source_sha256(
        path: impl AsRef<Path>,
        region_id: &str,
        sha256: &str,
    ) -> Result<(), String> {
        let path = path.as_ref();
        let region_id = region_id.trim().trim_matches('/');
        let sha256 = sha256.trim();
        if region_id.is_empty() {
            return Err("region_id empty".into());
        }
        if sha256.is_empty() {
            return Err("source sha256 empty".into());
        }
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        Self::apply_file_pragmas(&conn).map_err(|e| e.to_string())?;
        Self::ensure_source_sha256_column(&conn).map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "UPDATE name_index_build SET source_sha256 = ?1 WHERE region_id = ?2",
                params![sha256, region_id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            conn.execute(
                "INSERT INTO name_index_build(region_id, expected, written, complete, source_sha256)
                 SELECT ?2, COUNT(*), COUNT(*), 1, ?1
                 FROM name_entries WHERE region_id = ?2",
                params![sha256, region_id],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// How this region's index was built: `place-source`, `own-extract`, or none.
    pub fn place_index_source(path: impl AsRef<Path>, region_id: &str) -> Option<String> {
        let path = path.as_ref();
        let region_id = region_id.trim().trim_matches('/');
        if !path.is_file() || region_id.is_empty() {
            return None;
        }
        let Ok(conn) =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            return None;
        };
        conn.query_row(
            "SELECT index_source FROM name_index_build WHERE region_id = ?1",
            params![region_id],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    }

    /// Record `place-source` or `own-extract` without changing row counts.
    pub fn set_place_index_source(
        path: impl AsRef<Path>,
        region_id: &str,
        source: &str,
    ) -> Result<(), String> {
        let path = path.as_ref();
        let region_id = region_id.trim().trim_matches('/');
        let source = source.trim();
        if region_id.is_empty() {
            return Err("region_id empty".into());
        }
        if source != "place-source" && source != "own-extract" {
            return Err(format!("invalid index_source {source}"));
        }
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        Self::apply_file_pragmas(&conn).map_err(|e| e.to_string())?;
        Self::ensure_index_source_column(&conn).map_err(|e| e.to_string())?;
        let n = conn
            .execute(
                "UPDATE name_index_build SET index_source = ?1 WHERE region_id = ?2",
                params![source, region_id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            conn.execute(
                "INSERT INTO name_index_build(region_id, expected, written, complete, index_source)
                 SELECT ?2, COUNT(*), COUNT(*), 1, ?1
                 FROM name_entries WHERE region_id = ?2",
                params![source, region_id],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn osm_ids_for_region(conn: &Connection, region_id: &str) -> SqlResult<HashSet<i64>> {
        let mut stmt = conn.prepare("SELECT osm_id FROM name_entries WHERE region_id = ?1")?;
        let rows = stmt.query_map(params![region_id], |row| row.get(0))?;
        rows.collect()
    }

    fn upsert_build_progress(
        tx: &rusqlite::Transaction<'_>,
        region_id: &str,
        expected: i64,
        written: i64,
        complete: bool,
    ) -> SqlResult<()> {
        tx.execute(
            "INSERT INTO name_index_build(region_id, expected, written, complete)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(region_id) DO UPDATE SET
                expected = excluded.expected,
                written = excluded.written,
                complete = excluded.complete",
            params![region_id, expected, written, i64::from(complete)],
        )?;
        Ok(())
    }

    /// Insert or replace one name row (tests / incremental updates).
    pub fn upsert_entry(
        &mut self,
        osm_id: i64,
        name: String,
        kind: String,
        lat: f64,
        lon: f64,
    ) -> SqlResult<()> {
        self.upsert_entry_with_context(osm_id, name, kind, lat, lon, String::new(), String::new())
    }

    pub fn upsert_entry_with_context(
        &mut self,
        osm_id: i64,
        name: String,
        kind: String,
        lat: f64,
        lon: f64,
        sub_area: String,
        municipality: String,
    ) -> SqlResult<()> {
        self.upsert_entry_with_region(
            osm_id,
            name,
            kind,
            lat,
            lon,
            sub_area,
            municipality,
            String::new(),
        )
    }

    pub fn upsert_entry_with_region(
        &mut self,
        osm_id: i64,
        name: String,
        kind: String,
        lat: f64,
        lon: f64,
        sub_area: String,
        municipality: String,
        region_id: String,
    ) -> SqlResult<()> {
        // New writes store display name plus ASCII / umlaut fold aliases.
        self.conn.execute(
            "INSERT OR REPLACE INTO name_entries(osm_id, name, kind, lat, lon, sub_area, municipality, region_id, search_doc)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                osm_id,
                &name,
                &kind,
                lat,
                lon,
                &sub_area,
                &municipality,
                &region_id,
                &search_doc_with_fold_aliases(&name)
            ],
        )?;
        let rowid: i64 = self.conn.query_row(
            "SELECT rowid FROM name_entries WHERE region_id = ?1 AND osm_id = ?2",
            params![&region_id, osm_id],
            |row| row.get(0),
        )?;
        let _ = self.conn.execute(
            "INSERT INTO name_fts(name_fts, rowid, search_doc, kind) VALUES('delete', ?1, NULL, NULL)",
            params![rowid],
        );
        self.conn.execute(
            "INSERT INTO name_fts(rowid, search_doc, kind) VALUES (?1,?2,?3)",
            params![rowid, search_doc_with_fold_aliases(&name), &kind],
        )?;
        Ok(())
    }

    /// `name` has no index, so a plain `name = ?` scans every row (seconds on a
    /// phone). Every `search_doc` starts with the name's tokens, so a phrase
    /// anchored at the column start finds all candidates through FTS.
    fn exact_name_hits(&self, query: &str, limit: usize) -> SqlResult<Vec<NameHit>> {
        if !query.chars().any(char::is_alphanumeric) {
            return Ok(Vec::new());
        }
        let anchored = format!("search_doc : ^\"{}\"", query.replace('"', "\"\""));
        let mut stmt = self.conn.prepare(
            "
            SELECT e.osm_id, e.name, e.kind, e.lat, e.lon, e.sub_area, e.municipality, e.region_id, COALESCE(e.names_json, '')
            FROM name_fts f
            JOIN name_entries e ON e.rowid = f.rowid
            WHERE name_fts MATCH ?1 AND e.name = ?2 COLLATE NOCASE
            LIMIT ?3
            ",
        )?;
        let rows = stmt.query_map(params![anchored, query, limit as i64], |row| {
            Ok(name_hit_from_parts(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get::<_, String>(8).unwrap_or_default(),
            ))
        })?;
        rows.collect()
    }

    /// Rows whose folded name equals the folded query (`Kobenhavn` ->
    /// `København`). The prefix MATCH stops at `fetch` rows, so a crowded stem
    /// can push out the one place the user meant; this pass is not capped by it.
    fn folded_exact_hits(&self, query: &str, limit: usize) -> SqlResult<Vec<NameHit>> {
        const CANDIDATES: i64 = 5000;
        let variants = anchored_first_word_variants(query, 40);
        if variants.is_empty() {
            return Ok(Vec::new());
        }
        let want = fold_search_key(query);
        let mut match_q = format!(
            "({})",
            variants
                .iter()
                .map(|v| format!("search_doc : ^\"{v}\"*"))
                .collect::<Vec<_>>()
                .join(" OR ")
        );
        // House numbers cannot carry a folded letter; requiring them keeps a
        // common street stem (Storg*) from filling the candidate cap.
        for tok in want.split_whitespace().skip(1) {
            if tok.chars().all(|c| c.is_ascii_digit()) {
                match_q.push_str(&format!(" AND search_doc : \"{tok}\""));
            }
        }
        let mut stmt = self.conn.prepare(
            "
            SELECT e.osm_id, e.name, e.kind, e.lat, e.lon, e.sub_area, e.municipality, e.region_id, COALESCE(e.names_json, '')
            FROM name_fts f
            JOIN name_entries e ON e.rowid = f.rowid
            WHERE name_fts MATCH ?1
            LIMIT ?2
            ",
        )?;
        let rows = stmt.query_map(params![match_q, CANDIDATES], |row| {
            Ok(name_hit_from_parts(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get::<_, String>(8).unwrap_or_default(),
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            let h = r?;
            if fold_search_key(&h.name) == want {
                out.push(h);
                if out.len() >= limit {
                    break;
                }
            }
        }
        Ok(out)
    }

    pub fn search_biased(
        &self,
        query: &str,
        limit: usize,
        bias_lat: Option<f64>,
        bias_lon: Option<f64>,
        visible_region: &str,
    ) -> SqlResult<Vec<NameHit>> {
        let mut out = self.search(query, limit.saturating_mul(4).max(limit))?;
        let q_raw = query.trim();
        let visible = visible_region.trim().trim_matches('/');
        out.sort_by(|a, b| {
            search_rank_key(a, q_raw, Some((bias_lat, bias_lon, visible))).cmp(&search_rank_key(
                b,
                q_raw,
                Some((bias_lat, bias_lon, visible)),
            ))
        });
        out.truncate(limit);
        Ok(out)
    }

    fn search_fts_match(&self, match_q: &str, fetch: usize) -> SqlResult<Vec<NameHit>> {
        if match_q.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            "
            SELECT e.osm_id, e.name, e.kind, e.lat, e.lon, e.sub_area, e.municipality, e.region_id, COALESCE(e.names_json, '')
            FROM name_fts f
            JOIN name_entries e ON e.rowid = f.rowid
            WHERE name_fts MATCH ?1
            LIMIT ?2
            ",
        )?;
        let rows = stmt.query_map(params![match_q, fetch as i64], |row| {
            Ok(name_hit_from_parts(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get::<_, String>(8).unwrap_or_default(),
            ))
        })?;
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for r in rows {
            match r {
                Ok(h) => {
                    if seen.insert((h.region_id.clone(), h.osm_id)) {
                        out.push(h);
                    }
                }
                Err(e) => {
                    log::warn!(
                        target: "NaviSearch",
                        "name_fts row skipped: {e}"
                    );
                }
            }
        }
        Ok(out)
    }

    fn collect_search_hits(&self, query: &str, fetch: usize) -> SqlResult<Vec<NameHit>> {
        let mut seen = HashSet::new();
        let mut merged = Vec::new();
        let mut db_err: Option<rusqlite::Error> = None;
        let variants = search_query_variants(query);
        for variant in &variants {
            match self.search_fts_match(variant, fetch) {
                Ok(hits) => {
                    for h in hits {
                        if seen.insert((h.region_id.clone(), h.osm_id)) {
                            merged.push(h);
                        }
                    }
                }
                Err(e) => {
                    log::warn!(
                        target: "NaviSearch",
                        "name_fts MATCH failed: {e}"
                    );
                    db_err = Some(e);
                    break;
                }
            }
        }
        match self.exact_name_hits(query, 200) {
            Ok(hits) => {
                for h in hits {
                    if seen.insert((h.region_id.clone(), h.osm_id)) {
                        merged.push(h);
                    }
                }
            }
            Err(e) => {
                log::warn!(
                    target: "NaviSearch",
                    "exact name lookup failed: {e}"
                );
                if db_err.is_none() {
                    db_err = Some(e);
                }
            }
        }
        match self.folded_exact_hits(query, 200) {
            Ok(hits) => {
                for h in hits {
                    if seen.insert((h.region_id.clone(), h.osm_id)) {
                        merged.push(h);
                    }
                }
            }
            Err(e) => {
                log::warn!(
                    target: "NaviSearch",
                    "folded exact lookup failed: {e}"
                );
                if db_err.is_none() {
                    db_err = Some(e);
                }
            }
        }
        if let Some(or_q) = query_time_prefix_or_match(query) {
            match self.search_fts_match(&or_q, fetch) {
                Ok(hits) => {
                    for h in hits {
                        if seen.insert((h.region_id.clone(), h.osm_id)) {
                            merged.push(h);
                        }
                    }
                }
                Err(e) => {
                    log::warn!(
                        target: "NaviSearch",
                        "name_fts prefix MATCH failed: {e}"
                    );
                    if db_err.is_none() {
                        db_err = Some(e);
                    }
                }
            }
        }
        if let Some(e) = db_err {
            return Err(e);
        }
        Ok(merged)
    }

    pub fn search(&self, query: &str, limit: usize) -> SqlResult<Vec<NameHit>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let fetch = (limit.saturating_mul(8)).clamp(40, 200);
        let addr = parse_address_query(q);
        let (search_q, place_filter, want_number) = if let Some(addr) = addr.as_ref() {
            let street_q = match addr.number.as_deref() {
                Some(n) => format!("{} {}", addr.street, n),
                None => addr.street.clone(),
            };
            (street_q, addr.place.clone(), addr.number.clone())
        } else {
            (q.to_string(), None, None)
        };
        let place_key = place_filter.as_deref().map(fold_search_key);
        let in_place = |h: &NameHit| {
            place_key.as_deref().is_none_or(|pf| {
                fold_search_key(&h.municipality).contains(pf)
                    || fold_search_key(&h.sub_area).contains(pf)
                    || fold_search_key(&h.name).contains(pf)
            })
        };
        let mut merged = self.collect_search_hits(&search_q, fetch)?;
        if let Some(num) = want_number.as_deref() {
            let with_num: Vec<NameHit> = merged
                .iter()
                .filter(|h| {
                    house_number_matches(&h.name, num)
                        && hit_matches_all_tokens(h, &search_q)
                        && in_place(h)
                })
                .cloned()
                .collect();
            if with_num.is_empty() {
                let street = addr.as_ref().unwrap().street.as_str();
                let street_only = self.collect_search_hits(street, fetch)?;
                let mut streets_seen = HashSet::new();
                merged = street_only
                    .into_iter()
                    .filter(|h| hit_matches_all_tokens(h, street) && in_place(h))
                    .filter_map(|mut h| {
                        if h.kind.starts_with("addr:") || h.kind.starts_with("highway:") {
                            // One row per street and town, not one per house.
                            let key = (h.region_id.clone(), fold_search_key(&h.municipality));
                            if !streets_seen.insert(key) {
                                return None;
                            }
                            h.kind = "addr:street".into();
                            if !h.name.contains("house number") {
                                h.name = format!("{street} (house number {num} not in the index)");
                            }
                        }
                        Some(h)
                    })
                    .collect();
            } else {
                merged = with_num;
            }
        }
        merged.retain(|h| hit_matches_all_tokens(h, &search_q) && in_place(h));
        merged.sort_by(|a, b| search_rank_key(a, q, None).cmp(&search_rank_key(b, q, None)));
        merged.truncate(limit);
        Ok(merged)
    }

    /// Entries within [radius_m] of `(lat, lon)`, nearest first (Haversine).
    ///
    /// Uses a degree bbox prefilter then exact distance — fine for place-index
    /// sizes; not a substitute for a dedicated road-edge spatial index.
    pub fn nearby(
        &self,
        lat: f64,
        lon: f64,
        radius_m: f64,
        limit: usize,
    ) -> SqlResult<Vec<NameHit>> {
        let radius_m = radius_m.max(1.0);
        let limit = limit.max(1);
        let lat_pad = radius_m / 111_320.0;
        let lon_pad = radius_m / (111_320.0 * lat.to_radians().cos().max(0.2));
        let mut stmt = self.conn.prepare(
            "
            SELECT osm_id, name, kind, lat, lon, sub_area, municipality, region_id, COALESCE(names_json, '')
            FROM name_entries
            WHERE lat BETWEEN ?1 AND ?2 AND lon BETWEEN ?3 AND ?4
            ",
        )?;
        let rows = stmt.query_map(
            params![lat - lat_pad, lat + lat_pad, lon - lon_pad, lon + lon_pad],
            |row| {
                Ok(name_hit_from_parts(
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get::<_, String>(8).unwrap_or_default(),
                ))
            },
        )?;
        let mut scored: Vec<(f64, NameHit)> = Vec::new();
        for r in rows {
            let hit = r?;
            let d_m = crate::tracks::haversine_km(lat, lon, hit.lat, hit.lon) * 1000.0;
            if d_m <= radius_m {
                scored.push((d_m, hit));
            }
        }
        scored.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok(scored.into_iter().take(limit).map(|(_, h)| h).collect())
    }

    /// Named places of an exact `kind` inside a lat/lon bbox (inclusive).
    pub fn places_of_kind_in_bbox(
        &self,
        kind: &str,
        min_lat: f64,
        min_lon: f64,
        max_lat: f64,
        max_lon: f64,
        limit: usize,
    ) -> SqlResult<Vec<NameHit>> {
        let limit = limit.max(1);
        let mut stmt = self.conn.prepare(
            "
            SELECT osm_id, name, kind, lat, lon, sub_area, municipality, region_id, COALESCE(names_json, '')
            FROM name_entries
            WHERE kind = ?1
              AND lat BETWEEN ?2 AND ?3
              AND lon BETWEEN ?4 AND ?5
            ORDER BY name
            LIMIT ?6
            ",
        )?;
        let rows = stmt.query_map(
            params![kind, min_lat, max_lat, min_lon, max_lon, limit as i64],
            |row| {
                Ok(name_hit_from_parts(
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get::<_, String>(8).unwrap_or_default(),
                ))
            },
        )?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
}

/// Build the FTS search document from the primary display name plus OSM
/// `alt_name` / `loc_name` / localized `name:*` / `int_name`.
fn place_search_doc(
    primary: &str,
    alt_name: Option<&str>,
    loc_name: Option<&str>,
    localized_extras: &[String],
) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut push_tag = |raw: &str| {
        for part in raw.split(';') {
            let t = part.trim();
            if t.is_empty() {
                continue;
            }
            let key = t.to_lowercase();
            if seen.insert(key) {
                parts.push(t.to_string());
            }
        }
    };
    push_tag(primary);
    if let Some(a) = alt_name {
        push_tag(a);
    }
    if let Some(l) = loc_name {
        push_tag(l);
    }
    for extra in localized_extras {
        push_tag(extra);
    }
    for alias in fold_name_aliases(primary) {
        push_tag(&alias);
    }
    parts.join(" ")
}

/// Classify a tagged OSM element for the place index.
///
/// Returns `(osm_id, display_name, search_doc, kind, names_json, lat, lon)`.
/// Display prefers `name`, then `loc_name`, then the first `alt_name` part,
/// then address tags. FTS matches against `search_doc` (primary + alts +
/// localized names). `names_json` holds `name:*` / `int_name` for locale pick.
fn classify_named<'a>(
    osm_id: i64,
    lat: f64,
    lon: f64,
    tags: impl Iterator<Item = (&'a str, &'a str)>,
) -> Option<(i64, String, String, String, String, f64, f64)> {
    let mut name = None;
    let mut alt_name = None;
    let mut loc_name = None;
    let mut addr_street = None;
    let mut addr_housenumber = None;
    let mut kind = "named".to_string();
    let mut is_building = false;
    let mut loc_tags: Vec<(&str, &str)> = Vec::new();
    for (k, v) in tags {
        match k {
            "name" => name = Some(v.to_string()),
            "alt_name" => alt_name = Some(v.to_string()),
            "loc_name" => loc_name = Some(v.to_string()),
            "addr:street" => addr_street = Some(v.to_string()),
            "addr:housenumber" => addr_housenumber = Some(v.to_string()),
            "place" => kind = format!("place:{v}"),
            "tourism" => kind = format!("tourism:{v}"),
            "leisure" => kind = format!("leisure:{v}"),
            "natural" if v == "peak" => kind = "natural:peak".into(),
            "highway" => kind = format!("highway:{v}"),
            "amenity" => kind = format!("amenity:{v}"),
            "shop" => kind = format!("shop:{v}"),
            // Plain named footprints (building=* + name=*) — not amenity/shop.
            // Amenity/shop/place above win when present so POIs stay classified.
            "building" if !v.eq_ignore_ascii_case("no") => is_building = true,
            "int_name" => loc_tags.push((k, v)),
            k if k.starts_with("name:") && !k.starts_with("name:etymology") => {
                loc_tags.push((k, v));
            }
            _ => {}
        }
    }
    let names_json = localized_store::names_json_from_tags(loc_tags.into_iter());
    let localized_extras = localized_store::localized_search_tokens(&names_json);
    let mut display = name.or_else(|| loc_name.clone()).or_else(|| {
        alt_name.as_ref().and_then(|a| {
            a.split(';')
                .map(str::trim)
                .find(|p| !p.is_empty())
                .map(|p| p.to_string())
        })
    });
    if display.is_none() {
        if let (Some(street), Some(num)) = (addr_street.as_ref(), addr_housenumber.as_ref()) {
            display = Some(format!("{street} {num}"));
            if kind == "named" {
                kind = "addr:housenumber".into();
            }
        } else if let Some(street) = addr_street {
            display = Some(street);
            if kind == "named" {
                kind = "addr:street".into();
            }
        }
    }
    if kind == "named" && is_building {
        kind = NAMED_BUILDING_KIND.to_string();
    }
    let display = display?;
    let search_doc = place_search_doc(
        &display,
        alt_name.as_deref(),
        loc_name.as_deref(),
        &localized_extras,
    );
    Some((osm_id, display, search_doc, kind, names_json, lat, lon))
}

impl NameIndex {
    /// Named building footprints (`kind = building`) inside a lat/lon bbox.
    ///
    /// Requires a place index built at current [`PLACE_INDEX_SCHEMA_VERSION`]
    /// so `classify_named` stored buildings as [`NAMED_BUILDING_KIND`]. Older
    /// on-device indexes migrate in place on the next writer open — the shared
    /// file is never discarded.
    pub fn named_buildings_in_bbox(
        &self,
        min_lat: f64,
        min_lon: f64,
        max_lat: f64,
        max_lon: f64,
        limit: usize,
    ) -> SqlResult<Vec<NameHit>> {
        self.places_of_kind_in_bbox(
            NAMED_BUILDING_KIND,
            min_lat,
            min_lon,
            max_lat,
            max_lon,
            limit,
        )
    }
}

/// Saved route persistence (host UI route list).
#[derive(Debug, Clone)]
pub struct SavedRoute {
    pub id: String,
    pub start_lat: f64,
    pub start_lon: f64,
    pub start_name: Option<String>,
    pub end_lat: f64,
    pub end_lon: f64,
    pub end_name: Option<String>,
    pub via_json: String,
    pub profile: String,
    pub vehicle_json: String,
    pub summary_json: String,
    pub created_at: String,
    pub last_break_lat: Option<f64>,
    pub last_break_lon: Option<f64>,
    pub last_overnight_lat: Option<f64>,
    pub last_overnight_lon: Option<f64>,
}

pub struct RouteStore<'a> {
    storage: &'a Storage,
}

impl<'a> RouteStore<'a> {
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    pub fn insert(&self, route: &SavedRoute) -> SqlResult<()> {
        self.storage.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO routes(
                    id, start_lat, start_lon, start_name, end_lat, end_lon, end_name,
                    via_json, profile, vehicle_json, summary_json, created_at,
                    last_break_lat, last_break_lon, last_overnight_lat, last_overnight_lon
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
                params![
                    route.id,
                    route.start_lat,
                    route.start_lon,
                    route.start_name,
                    route.end_lat,
                    route.end_lon,
                    route.end_name,
                    route.via_json,
                    route.profile,
                    route.vehicle_json,
                    route.summary_json,
                    route.created_at,
                    route.last_break_lat,
                    route.last_break_lon,
                    route.last_overnight_lat,
                    route.last_overnight_lon,
                ],
            )?;
            Ok(())
        })
    }

    pub fn delete(&self, id: &str) -> SqlResult<()> {
        self.storage.with_conn(|conn| {
            conn.execute("DELETE FROM routes WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    pub fn get(&self, id: &str) -> SqlResult<Option<SavedRoute>> {
        self.storage.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, start_lat, start_lon, start_name, end_lat, end_lon, end_name,
                        via_json, profile, vehicle_json, summary_json, created_at,
                        last_break_lat, last_break_lon, last_overnight_lat, last_overnight_lon
                 FROM routes WHERE id = ?1",
            )?;
            let mut rows = stmt.query(params![id])?;
            let Some(row) = rows.next()? else {
                return Ok(None);
            };
            Ok(Some(SavedRoute {
                id: row.get(0)?,
                start_lat: row.get(1)?,
                start_lon: row.get(2)?,
                start_name: row.get(3)?,
                end_lat: row.get(4)?,
                end_lon: row.get(5)?,
                end_name: row.get(6)?,
                via_json: row.get(7)?,
                profile: row.get(8)?,
                vehicle_json: row.get(9)?,
                summary_json: row.get(10)?,
                created_at: row.get(11)?,
                last_break_lat: row.get(12)?,
                last_break_lon: row.get(13)?,
                last_overnight_lat: row.get(14)?,
                last_overnight_lon: row.get(15)?,
            }))
        })
    }

    pub fn list(&self) -> SqlResult<Vec<SavedRoute>> {
        self.storage.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, start_lat, start_lon, start_name, end_lat, end_lon, end_name,
                        via_json, profile, vehicle_json, summary_json, created_at,
                        last_break_lat, last_break_lon, last_overnight_lat, last_overnight_lon
                 FROM routes ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(SavedRoute {
                    id: row.get(0)?,
                    start_lat: row.get(1)?,
                    start_lon: row.get(2)?,
                    start_name: row.get(3)?,
                    end_lat: row.get(4)?,
                    end_lon: row.get(5)?,
                    end_name: row.get(6)?,
                    via_json: row.get(7)?,
                    profile: row.get(8)?,
                    vehicle_json: row.get(9)?,
                    summary_json: row.get(10)?,
                    created_at: row.get(11)?,
                    last_break_lat: row.get(12)?,
                    last_break_lon: row.get(13)?,
                    last_overnight_lat: row.get(14)?,
                    last_overnight_lon: row.get(15)?,
                })
            })?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
    }
}

/// Named single-coordinate place (distinct from a full saved route corridor).
#[derive(Debug, Clone)]
pub struct SavedPlace {
    pub id: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub kind: String,
    pub created_at: String,
}

pub struct PlaceStore<'a> {
    storage: &'a Storage,
}

impl<'a> PlaceStore<'a> {
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    pub fn insert(&self, place: &SavedPlace) -> SqlResult<()> {
        self.storage.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO saved_places(id, name, lat, lon, kind, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![
                    place.id,
                    place.name,
                    place.lat,
                    place.lon,
                    place.kind,
                    place.created_at,
                ],
            )?;
            Ok(())
        })
    }

    pub fn rename(&self, id: &str, name: &str) -> SqlResult<bool> {
        self.storage.with_conn(|conn| {
            let n = conn.execute(
                "UPDATE saved_places SET name = ?1 WHERE id = ?2",
                params![name, id],
            )?;
            Ok(n > 0)
        })
    }

    pub fn delete(&self, id: &str) -> SqlResult<()> {
        self.storage.with_conn(|conn| {
            conn.execute("DELETE FROM saved_places WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    pub fn list(&self) -> SqlResult<Vec<SavedPlace>> {
        self.storage.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, lat, lon, kind, created_at
                 FROM saved_places ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(SavedPlace {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    lat: row.get(2)?,
                    lon: row.get(3)?,
                    kind: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fts_matches_norwegian_special_chars() {
        let mut idx = NameIndex::open_in_memory().expect("mem index");
        idx.upsert_entry(1, "Mjøsvegen".into(), "highway:tertiary".into(), 60.8, 11.0)
            .unwrap();
        idx.upsert_entry(2, "Trollåsveien".into(), "named".into(), 59.8, 10.8)
            .unwrap();
        idx.upsert_entry(
            3,
            "Ævongsli 2".into(),
            "addr:housenumber".into(),
            61.0,
            11.2,
        )
        .unwrap();
        idx.upsert_entry(
            4,
            "Bjørnhollia".into(),
            "tourism:alpine_hut".into(),
            61.7,
            10.0,
        )
        .unwrap();

        let mjos = idx.search("Mjøs", 8).unwrap();
        assert!(
            mjos.iter().any(|h| h.name.contains('ø')),
            "expected ø hit, got {mjos:?}"
        );
        let aas = idx.search("Trollås", 8).unwrap();
        assert!(aas.iter().any(|h| h.name.contains('å')), "got {aas:?}");
        let ae = idx.search("Ævongsli", 8).unwrap();
        assert!(!ae.is_empty(), "æ/Æ query empty");
        let bj = idx.search("Bjørn", 8).unwrap();
        assert!(bj.iter().any(|h| h.name.contains('ø')), "got {bj:?}");
    }

    #[test]
    fn keep_both_regions_on_osm_id_clash() {
        let mut idx = NameIndex::open_in_memory().expect("mem index");
        idx.upsert_entry_with_region(
            42,
            "Border".into(),
            "place:hamlet".into(),
            55.0,
            12.0,
            String::new(),
            String::new(),
            "europe/denmark".into(),
        )
        .unwrap();
        idx.upsert_entry_with_region(
            42,
            "Stolen".into(),
            "place:hamlet".into(),
            53.5,
            10.0,
            String::new(),
            String::new(),
            "europe/germany/hamburg".into(),
        )
        .unwrap();
        let n: i64 = idx
            .conn
            .query_row(
                "SELECT COUNT(*) FROM name_entries WHERE osm_id = 42",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(n, 2);
        let hits = idx.search("Stolen", 8).unwrap();
        assert_eq!(hits.iter().filter(|h| h.osm_id == 42).count(), 1);
    }

    #[test]
    fn same_region_osm_id_replace_allowed() {
        let mut idx = NameIndex::open_in_memory().expect("mem index");
        idx.upsert_entry_with_region(
            7,
            "Old".into(),
            "place:hamlet".into(),
            61.0,
            7.0,
            String::new(),
            String::new(),
            "europe/norway/vestlandet".into(),
        )
        .unwrap();
        idx.upsert_entry_with_region(
            7,
            "New".into(),
            "place:village".into(),
            61.1,
            7.1,
            String::new(),
            String::new(),
            "europe/norway/vestlandet".into(),
        )
        .unwrap();
        let name: String = idx
            .conn
            .query_row(
                "SELECT name FROM name_entries WHERE osm_id = 7",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(name, "New");
    }

    /// Document SQLite FTS5 unicode61 folding: å/ü fold to ASCII bases; æ/ø do not.
    #[test]
    fn fts_unicode61_folds_aa_but_not_ae_oe() {
        let mut idx = NameIndex::open_in_memory().expect("mem index");
        idx.upsert_entry(1, "Eldåbu".into(), "tourism:alpine_hut".into(), 61.75, 9.97)
            .unwrap();
        idx.upsert_entry(2, "Bærums Verk".into(), "place:suburb".into(), 59.94, 10.50)
            .unwrap();
        idx.upsert_entry(3, "Løten".into(), "place:village".into(), 60.82, 11.34)
            .unwrap();
        idx.upsert_entry(4, "Müllerstraße".into(), "named".into(), 52.5, 13.4)
            .unwrap();

        // Stored glyphs unchanged.
        assert_eq!(idx.search("Eldåbu", 1).unwrap()[0].name, "Eldåbu");
        assert!(idx.search("Bærum", 1).unwrap()[0].name.contains('æ'));
        assert!(idx.search("Løten", 1).unwrap()[0].name.contains('ø'));

        // å / ü fold → ASCII keyboard can find them.
        assert!(
            idx.search("Eldabu", 8)
                .unwrap()
                .iter()
                .any(|h| h.name == "Eldåbu"),
            "å should fold to a"
        );
        assert!(
            idx.search("Muller", 8)
                .unwrap()
                .iter()
                .any(|h| h.name.contains('ü') || h.name.contains("Muller")),
            "ü should fold to u"
        );

        // Fold aliases: ASCII approximations find æ / ø after search_doc aliases.
        assert!(
            idx.search("Baerum", 8)
                .unwrap()
                .iter()
                .any(|h| h.name.contains('æ')),
            "æ should match ae after fold aliases"
        );
        assert!(
            idx.search("Loten", 8)
                .unwrap()
                .iter()
                .any(|h| h.name == "Løten"),
            "ø should match plain o after fold aliases"
        );
    }

    #[test]
    fn fold_search_key_covers_required_spellings() {
        assert_eq!(fold_search_key("Dombås"), fold_search_key("Dombas"));
        assert_eq!(fold_search_key("dombås"), fold_search_key("DOMBÅS"));
        assert_eq!(fold_search_key("Åre"), fold_search_key("Are"));
        assert_eq!(fold_search_key("Øra"), fold_search_key("Ora"));
        assert_eq!(fold_search_key("København"), fold_search_key("Kobenhavn"));
        assert_eq!(fold_search_key("Århus"), fold_search_key("Arhus"));
        assert_eq!(fold_search_key("Łódź"), fold_search_key("Lodz"));
        assert_eq!(fold_search_key("München"), fold_search_key("Munchen"));
        assert_eq!(fold_search_key("Köln"), fold_search_key("Koln"));
        assert_eq!(fold_search_key("Görlitz"), fold_search_key("Gorlitz"));
        assert_eq!(
            fold_search_key("Świnoujście"),
            fold_search_key("Swinoujscie")
        );
        // precomposed vs combining ring
        let nfc = "Dombås";
        let nfd = "Domba\u{030a}s";
        assert_eq!(fold_search_key(nfc), fold_search_key(nfd));
        assert!(fold_name_aliases("München")
            .iter()
            .any(|a| a.contains("ue") || a.contains("munchen")));
        assert!(fold_name_aliases("Århus")
            .iter()
            .any(|a| a.contains("aa") || a.contains("arhus")));
        assert_eq!(fold_search_key("Little A'Le'Inn").contains("little"), true);
    }

    #[test]
    fn search_finds_folded_and_case_variants() {
        let mut idx = NameIndex::open_in_memory().unwrap();
        idx.upsert_entry_with_region(
            1,
            "Dombås".into(),
            "place:village".into(),
            62.07,
            9.12,
            String::new(),
            "Dovre".into(),
            "europe/norway/ostlandet".into(),
        )
        .unwrap();
        idx.upsert_entry_with_region(
            2,
            "Åre".into(),
            "place:town".into(),
            63.40,
            13.08,
            String::new(),
            "Åre".into(),
            "europe/sweden/jamtland".into(),
        )
        .unwrap();
        idx.upsert_entry_with_region(
            3,
            "Are".into(),
            "place:farm".into(),
            61.0,
            7.0,
            String::new(),
            String::new(),
            "europe/norway/vestlandet".into(),
        )
        .unwrap();
        idx.upsert_entry_with_region(
            4,
            "Øra".into(),
            "place:suburb".into(),
            59.2,
            10.95,
            String::new(),
            "Fredrikstad".into(),
            "europe/norway/ostlandet".into(),
        )
        .unwrap();
        idx.upsert_entry_with_region(
            5,
            "København".into(),
            "place:city".into(),
            55.68,
            12.57,
            String::new(),
            String::new(),
            "europe/denmark".into(),
        )
        .unwrap();
        idx.upsert_entry_with_region(
            6,
            "København".into(),
            "place:farm".into(),
            60.8,
            10.7,
            "Brattberg".into(),
            "Gjøvik".into(),
            "europe/norway/ostlandet".into(),
        )
        .unwrap();
        idx.upsert_entry_with_region(
            7,
            "Łódź".into(),
            "place:city".into(),
            51.76,
            19.46,
            String::new(),
            String::new(),
            "europe/poland".into(),
        )
        .unwrap();
        idx.upsert_entry_with_region(
            8,
            "München".into(),
            "place:city".into(),
            48.14,
            11.58,
            String::new(),
            String::new(),
            "europe/germany/bayern".into(),
        )
        .unwrap();
        idx.upsert_entry_with_region(
            9,
            "Köln".into(),
            "place:city".into(),
            50.94,
            6.96,
            String::new(),
            String::new(),
            "europe/germany/nordrhein-westfalen".into(),
        )
        .unwrap();
        idx.upsert_entry_with_region(
            10,
            "Görlitz".into(),
            "place:town".into(),
            51.16,
            14.99,
            String::new(),
            String::new(),
            "europe/germany/sachsen".into(),
        )
        .unwrap();
        idx.upsert_entry_with_region(
            11,
            "Świnoujście".into(),
            "place:town".into(),
            53.91,
            14.25,
            String::new(),
            String::new(),
            "europe/poland".into(),
        )
        .unwrap();
        idx.upsert_entry_with_region(
            12,
            "Little A'Le'Inn".into(),
            "tourism:hotel".into(),
            37.64,
            -115.78,
            "Rachel".into(),
            String::new(),
            "north-america/us/nevada".into(),
        )
        .unwrap();
        idx.upsert_entry_with_region(
            17,
            "Rødby".into(),
            "place:town".into(),
            54.69,
            11.39,
            String::new(),
            String::new(),
            "europe/denmark".into(),
        )
        .unwrap();
        idx.upsert_entry_with_region(
            18,
            "Tønsberg".into(),
            "place:town".into(),
            59.27,
            10.41,
            String::new(),
            String::new(),
            "europe/norway/ostlandet".into(),
        )
        .unwrap();
        idx.upsert_entry_with_region(
            19,
            "Høyanger".into(),
            "place:town".into(),
            61.17,
            6.06,
            String::new(),
            String::new(),
            "europe/norway/vestlandet".into(),
        )
        .unwrap();
        idx.upsert_entry_with_region(
            20,
            "Ålesund".into(),
            "place:city".into(),
            62.47,
            6.15,
            String::new(),
            String::new(),
            "europe/norway/vestlandet".into(),
        )
        .unwrap();
        for q in ["Dombås", "Dombas", "dombås", "DOMBÅS", "Domba\u{030a}s"] {
            let hits = idx.search(q, 8).unwrap();
            assert!(
                !hits.is_empty() && hits[0].name == "Dombås",
                "q={q} variants={:?} hits={:?}",
                search_query_variants(q),
                hits.iter().map(|h| h.name.as_str()).collect::<Vec<_>>()
            );
        }
        let are = idx.search("Are", 8).unwrap();
        assert_eq!(are[0].name, "Åre", "town must beat farm for Are");
        assert_eq!(idx.search("Ora", 8).unwrap()[0].name, "Øra");
        assert_eq!(idx.search("Kobenhavn", 8).unwrap()[0].name, "København");
        assert_eq!(idx.search("København", 8).unwrap()[0].kind, "place:city");
        assert_eq!(idx.search("Aarhus", 8).unwrap().len(), 0); // no filler
        idx.upsert_entry_with_region(
            13,
            "Århus".into(),
            "place:city".into(),
            56.16,
            10.20,
            String::new(),
            String::new(),
            "europe/denmark".into(),
        )
        .unwrap();
        assert_eq!(idx.search("Aarhus", 8).unwrap()[0].name, "Århus");
        assert_eq!(idx.search("Lodz", 8).unwrap()[0].name, "Łódź");
        assert_eq!(idx.search("Munchen", 8).unwrap()[0].name, "München");
        assert_eq!(idx.search("Muenchen", 8).unwrap()[0].name, "München");
        assert_eq!(idx.search("Koln", 8).unwrap()[0].name, "Köln");
        assert_eq!(idx.search("Gorlitz", 8).unwrap()[0].name, "Görlitz");
        assert_eq!(idx.search("Swinoujscie", 8).unwrap()[0].name, "Świnoujście");
        assert_eq!(idx.search("Rodby", 8).unwrap()[0].name, "Rødby");
        assert_eq!(idx.search("Tonsberg", 8).unwrap()[0].name, "Tønsberg");
        assert_eq!(idx.search("Hoyanger", 8).unwrap()[0].name, "Høyanger");
        assert_eq!(idx.search("Alesund", 8).unwrap()[0].name, "Ålesund");
        assert_eq!(idx.search("Aalesund", 8).unwrap()[0].name, "Ålesund");
        assert!(idx
            .search("Little A'Le'Inn", 8)
            .unwrap()
            .iter()
            .any(|h| h.name.contains("A'Le'Inn")));
        let t0 = std::time::Instant::now();
        let _ = idx.search("Kobenhavn", 8).unwrap();
        let koben_ms = t0.elapsed().as_millis();
        assert!(
            koben_ms < 300,
            "Kobenhavn query-time fold {koben_ms} ms exceeds 300 ms"
        );
    }

    fn seed_fold_compare_fixture(idx: &mut NameIndex) {
        let rows = [
            (
                13707878i64,
                "København",
                "place:city",
                55.686724,
                12.570072,
                "Nørrevold",
                "Københavns Kommune",
                "europe/denmark",
            ),
            (
                9660615548,
                "København",
                "place:farm",
                60.960261,
                10.547924,
                "Brattberg",
                "Gjøvik",
                "europe/norway/ostlandet",
            ),
            (
                101,
                "Dombås",
                "place:village",
                62.075,
                9.128,
                "",
                "Dovre",
                "europe/norway/ostlandet",
            ),
            (
                102,
                "Åre",
                "place:town",
                63.398,
                13.081,
                "",
                "Åre",
                "europe/sweden/jamtland",
            ),
            (
                110,
                "Øra",
                "place:suburb",
                59.2,
                10.95,
                "",
                "Fredrikstad",
                "europe/norway/ostlandet",
            ),
            (
                103,
                "Łódź",
                "place:city",
                51.77,
                19.46,
                "",
                "",
                "europe/poland",
            ),
            (
                104,
                "München",
                "place:city",
                48.14,
                11.58,
                "",
                "",
                "europe/germany",
            ),
            (
                105,
                "Rødby",
                "place:town",
                54.69,
                11.39,
                "",
                "",
                "europe/denmark",
            ),
            (
                106,
                "Tønsberg",
                "place:town",
                59.27,
                10.41,
                "",
                "",
                "europe/norway/ostlandet",
            ),
            (
                107,
                "Høyanger",
                "place:town",
                61.17,
                6.06,
                "",
                "",
                "europe/norway/vestlandet",
            ),
            (
                108,
                "Ålesund",
                "place:city",
                62.47,
                6.15,
                "",
                "",
                "europe/norway/vestlandet",
            ),
            (
                109,
                "Świnoujście",
                "place:town",
                53.91,
                14.25,
                "",
                "",
                "europe/poland",
            ),
        ];
        for (osm, name, kind, lat, lon, sub, muni, rid) in rows {
            idx.upsert_entry_with_region(
                osm,
                name.into(),
                kind.into(),
                lat,
                lon,
                sub.into(),
                muni.into(),
                rid.into(),
            )
            .unwrap();
        }
    }

    fn hit_keys(hits: &[NameHit]) -> Vec<(i64, String, String)> {
        let mut keys: Vec<(i64, String, String)> = hits
            .iter()
            .map(|h| (h.osm_id, h.name.clone(), h.kind.clone()))
            .collect();
        keys.sort();
        keys
    }

    #[test]
    fn query_time_and_folded_keys_return_same_hits() {
        let mut unfolded = NameIndex::open_in_memory().unwrap();
        seed_fold_compare_fixture(&mut unfolded);
        unfolded
            .conn
            .execute("UPDATE name_entries SET search_doc = name", [])
            .unwrap();
        unfolded
            .conn
            .execute_batch("INSERT INTO name_fts(name_fts) VALUES('rebuild');")
            .unwrap();
        let unfolded_doc: String = unfolded
            .conn
            .query_row(
                "SELECT search_doc FROM name_entries WHERE osm_id = 101",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(unfolded_doc, "Dombås");

        let mut folded = NameIndex::open_in_memory().unwrap();
        seed_fold_compare_fixture(&mut folded);
        let folded_doc: String = folded
            .conn
            .query_row(
                "SELECT search_doc FROM name_entries WHERE osm_id = 101",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            folded_doc.contains("Dombas") || folded_doc.contains("dombas"),
            "new build stores folded keys, got {folded_doc}"
        );

        for q in [
            "Dombas",
            "Dombås",
            "Kobenhavn",
            "København",
            "Are",
            "Åre",
            "Ora",
            "Lodz",
            "Munchen",
            "Muenchen",
            "Rodby",
            "Tonsberg",
            "Hoyanger",
            "Alesund",
            "Aalesund",
            "Swinoujscie",
        ] {
            let a = hit_keys(&unfolded.search(q, 8).unwrap());
            let b = hit_keys(&folded.search(q, 8).unwrap());
            assert_eq!(a, b, "query {q}: unfolded={a:?} folded={b:?}");
            assert!(!a.is_empty(), "query {q} returned no hits");
        }
    }

    /// Tablet `constraint failed` is FTS5 UNIQUE on `name_fts` rowid, not a
    /// unique folded name. Two places named København (city vs farm) are both
    /// valid; the clash is inserting the same content-rowid twice when a NULL
    /// delete left the old FTS row in place (Android SQLite).
    #[test]
    fn fold_null_fts_delete_after_content_update_hits_unique() {
        let mut idx = NameIndex::open_in_memory().unwrap();
        idx.upsert_entry_with_region(
            13707878,
            "København".into(),
            "place:city".into(),
            55.686724,
            12.570072,
            "Nørrevold".into(),
            "Københavns Kommune".into(),
            "europe/denmark".into(),
        )
        .unwrap();
        idx.upsert_entry_with_region(
            9660615548,
            "København".into(),
            "place:farm".into(),
            60.960261,
            10.547924,
            "Brattberg".into(),
            "Gjøvik".into(),
            "europe/norway/ostlandet".into(),
        )
        .unwrap();
        idx.conn
            .execute("UPDATE name_entries SET search_doc = name", [])
            .unwrap();
        idx.conn
            .execute_batch("INSERT INTO name_fts(name_fts) VALUES('rebuild');")
            .unwrap();
        let same_name: i64 = idx
            .conn
            .query_row(
                "SELECT COUNT(*) FROM name_entries WHERE name = 'København'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            same_name, 2,
            "fixture is two København rows, not a name UNIQUE"
        );
        // Host rusqlite FTS5 content= tables replace on the same rowid; Android
        // SQLite on the tablet raises UNIQUE. Reproduce the constraint on a
        // standalone FTS table (same rowid, two INSERTs, no delete).
        idx.conn
            .execute_batch("CREATE VIRTUAL TABLE fts_clash USING fts5(search_doc, kind);")
            .unwrap();
        idx.conn
            .execute(
                "INSERT INTO fts_clash(rowid, search_doc, kind) VALUES (10556863, 'København', 'place:farm')",
                [],
            )
            .unwrap();
        let err = idx
            .conn
            .execute(
                "INSERT INTO fts_clash(rowid, search_doc, kind) VALUES (10556863, 'København Kobenhavn', 'place:farm')",
                [],
            )
            .expect_err("duplicate FTS rowid must hit UNIQUE");
        let msg = err.to_string();
        assert!(
            msg.to_lowercase().contains("constraint") || msg.to_lowercase().contains("unique"),
            "expected UNIQUE/constraint on FTS rowid, got {msg}"
        );
    }

    #[test]
    fn search_reports_database_error_when_fts_is_unreadable() {
        let mut idx = NameIndex::open_in_memory().unwrap();
        idx.upsert_entry_with_region(
            13707878,
            "København".into(),
            "place:city".into(),
            55.686724,
            12.570072,
            "Nørrevold".into(),
            "Københavns Kommune".into(),
            "europe/denmark".into(),
        )
        .unwrap();
        idx.conn.execute_batch("DROP TABLE name_fts;").unwrap();
        let err = idx
            .search("København", 8)
            .expect_err("damaged index must not look like zero hits");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("fts") || msg.contains("no such") || msg.contains("malformed"),
            "got {msg}"
        );
    }

    #[test]
    fn exact_name_lookup_goes_through_fts_not_a_table_scan() {
        let mut idx = NameIndex::open_in_memory().unwrap();
        for i in 0..300 {
            idx.upsert_entry_with_region(
                1000 + i,
                format!("Oslo Gate {i}"),
                "highway:residential".into(),
                59.9,
                10.7,
                String::new(),
                String::new(),
                "europe/norway/ostlandet".into(),
            )
            .unwrap();
        }
        for (id, name) in [
            (1, "Oslo"),
            (2, "Little A'Le'Inn"),
            (3, "Bar \"Nord\""),
            (4, "Øra"),
        ] {
            idx.upsert_entry_with_region(
                id,
                name.into(),
                "place:town".into(),
                59.9,
                10.7,
                String::new(),
                String::new(),
                "europe/norway/ostlandet".into(),
            )
            .unwrap();
        }
        let names = |q: &str| -> Vec<String> {
            idx.exact_name_hits(q, 200)
                .unwrap()
                .into_iter()
                .map(|h| h.name)
                .collect()
        };
        assert_eq!(names("oslo"), vec!["Oslo"]);
        assert_eq!(names("Little A'Le'Inn"), vec!["Little A'Le'Inn"]);
        assert_eq!(names("Bar \"Nord\""), vec!["Bar \"Nord\""]);
        assert_eq!(names("Øra"), vec!["Øra"]);
        assert!(names("Gate").is_empty());
        assert!(names("--").is_empty());
        assert_eq!(idx.search("Oslo", 8).unwrap()[0].name, "Oslo");

        let mut plan = idx
            .conn
            .prepare(
                "EXPLAIN QUERY PLAN
                 SELECT e.name FROM name_fts f JOIN name_entries e ON e.rowid = f.rowid
                 WHERE name_fts MATCH 'search_doc : ^\"Oslo\"' AND e.name = 'Oslo' COLLATE NOCASE",
            )
            .unwrap();
        let details: Vec<String> = plan
            .query_map([], |r| r.get::<_, String>(3))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert!(
            details.iter().all(|d| !d.starts_with("SCAN e")),
            "name_entries must not be scanned: {details:?}"
        );
    }

    #[test]
    fn house_number_on_another_street_does_not_hide_the_street() {
        let mut idx = NameIndex::open_in_memory().unwrap();
        for (id, name, municipality, region) in [
            (1, "hjulsta torg 1", "Stockholm", "europe/sweden/halland"),
            (
                2,
                "Torgallmenningen 1A",
                "Bergen",
                "europe/norway/vestlandet",
            ),
            (
                3,
                "Torgallmenningen 2",
                "Bergen",
                "europe/norway/vestlandet",
            ),
        ] {
            idx.upsert_entry_with_region(
                id,
                name.into(),
                "addr:housenumber".into(),
                60.39,
                5.32,
                String::new(),
                municipality.into(),
                region.into(),
            )
            .unwrap();
        }
        let with_place = idx.search("Torgallmenningen 1, Bergen", 8).unwrap();
        assert!(
            with_place[0]
                .name
                .contains("house number 1 not in the index"),
            "got {with_place:?}"
        );
        assert_eq!(
            with_place
                .iter()
                .filter(|h| h.name.contains("house number"))
                .count(),
            1,
            "one street row, not one per house: {with_place:?}"
        );
        let bare = idx.search("Torgallmenningen 1", 8).unwrap();
        assert!(
            bare.iter().all(|h| !h.name.contains("torg 1")),
            "another street matched: {bare:?}"
        );
        assert_eq!(
            fts_prefix_from_tokens(&["Storgata".into(), "1".into()]),
            "\"Storgata\"* \"1\""
        );
    }

    #[test]
    fn folded_exact_name_survives_a_crowded_prefix() {
        let mut idx = NameIndex::open_in_memory().unwrap();
        for i in 0..300 {
            idx.upsert_entry_with_region(
                5000 + i,
                format!("Københavns Gade {i}"),
                "highway:residential".into(),
                55.68,
                12.57,
                String::new(),
                String::new(),
                "europe/denmark".into(),
            )
            .unwrap();
        }
        idx.upsert_entry_with_region(
            1,
            "København".into(),
            "place:farm".into(),
            60.8,
            10.69,
            "Brattberg".into(),
            "Gjøvik".into(),
            "europe/norway/ostlandet".into(),
        )
        .unwrap();
        assert!(anchored_first_word_variants("Kobenhavn", 40)
            .iter()
            .any(|v| v == "københ"));
        let hits = idx.search("Kobenhavn", 8).unwrap();
        assert_eq!(hits[0].name, "København", "got {hits:?}");
        assert_eq!(hits[0].region_id, "europe/norway/ostlandet");
    }

    #[test]
    fn query_time_prefix_is_one_or_match() {
        let q = query_time_prefix_or_match("Kobenhavn").expect("prefix");
        assert!(q.contains(" OR "), "{q}");
        assert!(q.contains("\"Købe\"*") || q.contains("\"købe\"*"), "{q}");
        assert_eq!(q.matches(" OR ").count() + 1, q.split(" OR ").count());
        let one = query_time_prefix_or_match("Kobenhavn").unwrap();
        assert!(!one.contains('\n'));
    }

    #[test]
    fn address_forms_and_missing_house_number() {
        let mut idx = NameIndex::open_in_memory().unwrap();
        idx.upsert_entry_with_region(
            1,
            "Vågåvegen 80".into(),
            "addr:housenumber".into(),
            61.88,
            9.09,
            String::new(),
            "Vågå".into(),
            "europe/norway/ostlandet".into(),
        )
        .unwrap();
        idx.upsert_entry_with_region(
            2,
            "Storgata".into(),
            "highway:residential".into(),
            61.12,
            10.46,
            String::new(),
            "Lillehammer".into(),
            "europe/norway/ostlandet".into(),
        )
        .unwrap();
        let a = idx.search("Vågåvegen 80, Vågå", 8).unwrap();
        assert_eq!(a[0].name, "Vågåvegen 80");
        let b = idx.search("Vågåvegen 80 Vågå", 8).unwrap();
        assert_eq!(b[0].name, "Vågåvegen 80");
        let c = idx.search("Vågå, Vågåvegen 80", 8).unwrap();
        assert_eq!(c[0].name, "Vågåvegen 80");
        let missing = idx.search("Storgata 1 Lillehammer", 8).unwrap();
        assert!(
            missing[0].name.contains("house number 1 not in the index"),
            "got {:?}",
            missing[0].name
        );
        let nothing = idx.search("Xyzzyq NoSuchPlace 99", 8).unwrap();
        assert!(nothing.is_empty(), "no filler, got {nothing:?}");
    }

    #[test]
    fn parse_address_three_forms() {
        let a = parse_address_query("Vågåvegen 80, Vågå").unwrap();
        assert_eq!(a.street, "Vågåvegen");
        assert_eq!(a.number.as_deref(), Some("80"));
        assert_eq!(a.place.as_deref(), Some("Vågå"));
        let b = parse_address_query("Vågåvegen 80 Vågå").unwrap();
        assert_eq!(b.street, "Vågåvegen");
        assert_eq!(b.number.as_deref(), Some("80"));
        assert_eq!(b.place.as_deref(), Some("Vågå"));
        let c = parse_address_query("Vågå, Vågåvegen 80").unwrap();
        assert_eq!(c.street, "Vågåvegen");
        assert_eq!(c.number.as_deref(), Some("80"));
        assert_eq!(c.place.as_deref(), Some("Vågå"));
    }

    #[test]
    fn nearby_finds_peer_gyntvegen_address() {
        let mut idx = NameIndex::open_in_memory().expect("mem");
        idx.upsert_entry(
            1,
            "Peer Gyntvegen 1377".into(),
            "addr:housenumber".into(),
            61.420522,
            9.927719,
        )
        .unwrap();
        idx.upsert_entry(
            2,
            "Steinbrotvegen 4".into(),
            "addr:housenumber".into(),
            61.420086,
            9.927864,
        )
        .unwrap();
        let hits = idx.nearby(61.419774, 9.927647, 120.0, 8).expect("nearby");
        assert!(!hits.is_empty());
        assert!(
            hits.iter().any(|h| h.name.starts_with("Peer Gyntvegen")),
            "expected Peer Gyntvegen near fix, got {hits:?}"
        );
    }

    #[test]
    fn place_store_insert_list_rename_delete() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("navi.db");
        let storage = crate::storage::Storage::open(&db).expect("open");
        let store = PlaceStore::new(&storage);
        store
            .insert(&SavedPlace {
                id: "p1".into(),
                name: "Cabin ridge".into(),
                lat: 61.85,
                lon: 10.23,
                kind: "map-mark".into(),
                created_at: "100".into(),
            })
            .unwrap();
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Cabin ridge");
        assert!(store.rename("p1", "Atnbrufossen lookout").unwrap());
        assert_eq!(store.list().unwrap()[0].name, "Atnbrufossen lookout");
        store.delete("p1").unwrap();
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn route_store_get_and_gpx_export_roundtrip() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("navi.db");
        let storage = crate::storage::Storage::open(&db).expect("open");
        let store = RouteStore::new(&storage);
        let via_json = r#"[{"name":"Via","lat":61.2,"lon":10.4}]"#;
        store
            .insert(&SavedRoute {
                id: "r1".into(),
                start_lat: 61.1,
                start_lon: 10.5,
                start_name: Some("Start".into()),
                end_lat: 61.3,
                end_lon: 10.2,
                end_name: Some("End".into()),
                via_json: via_json.into(),
                profile: "car".into(),
                vehicle_json: "{}".into(),
                summary_json: "{}".into(),
                created_at: "2026-09-04T08:00:00Z".into(),
                last_break_lat: None,
                last_break_lon: None,
                last_overnight_lat: None,
                last_overnight_lon: None,
            })
            .unwrap();
        let got = store.get("r1").unwrap().expect("row");
        assert_eq!(got.start_name.as_deref(), Some("Start"));
        let rte = crate::export::route_points_from_saved(
            got.start_lat,
            got.start_lon,
            got.start_name.as_deref(),
            got.end_lat,
            got.end_lon,
            got.end_name.as_deref(),
            &got.via_json,
        );
        assert_eq!(rte.len(), 3);
        let poly = "10.500000,61.100000;10.400000,61.200000;10.200000,61.300000";
        let track = crate::export::parse_route_polyline(poly);
        let xml = crate::export::to_gpx(Some("Start -> End"), Some(&got.created_at), &rte, &track);
        assert!(xml.contains("<rtept"));
        assert_eq!(xml.matches("<rtept").count(), 3);
        assert_eq!(xml.matches("<trkpt").count(), 3);
        for pt in &rte {
            let needle = format!(r#"lat="{:.6}" lon="{:.6}""#, pt.lat, pt.lon);
            assert!(xml.contains(&needle), "missing {needle}");
        }
        assert!(store.get("missing").unwrap().is_none());
    }

    #[test]
    fn search_returns_precomputed_area_context() {
        let mut idx = NameIndex::open_in_memory().expect("mem");
        idx.upsert_entry_with_context(
            1,
            "Båberg".into(),
            "place:farm".into(),
            60.96849,
            10.54821,
            "Brattberg".into(),
            "Gjøvik".into(),
        )
        .unwrap();
        idx.upsert_entry_with_context(
            2,
            "Båberg".into(),
            "place:farm".into(),
            60.92416,
            10.83636,
            "Løken".into(),
            "Ringsaker".into(),
        )
        .unwrap();
        let hits = idx.search("Båberg", 8).unwrap();
        assert_eq!(hits.len(), 2);
        let labels: Vec<String> = hits
            .iter()
            .map(|h| format_place_display(&h.name, &h.sub_area, &h.municipality))
            .collect();
        assert!(
            labels.iter().any(|s| s == "Båberg, Brattberg, Gjøvik"),
            "got {labels:?}"
        );
        assert!(
            labels.iter().any(|s| s == "Båberg, Løken, Ringsaker"),
            "got {labels:?}"
        );
    }

    #[test]
    fn migrate_adds_context_columns_to_legacy_table() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("legacy.db");
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute_batch(
                "
                CREATE TABLE name_entries (
                    osm_id INTEGER PRIMARY KEY NOT NULL,
                    name TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    lat REAL NOT NULL,
                    lon REAL NOT NULL
                );
                CREATE VIRTUAL TABLE name_fts USING fts5(
                    name, kind, content='name_entries', content_rowid='osm_id'
                );
                ",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO name_entries(osm_id, name, kind, lat, lon) VALUES (1,'Tangen','place:village',60.6,11.2)",
                [],
            )
            .unwrap();
        }
        assert!(!NameIndex::is_current_schema(&db));
        let idx = NameIndex::open(&db).expect("open legacy");
        let hits = idx.nearby(60.6, 11.2, 500.0, 4).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "Tangen");
        assert_eq!(hits[0].municipality, "");
        assert_eq!(hits[0].sub_area, "");
    }

    #[test]
    fn discard_if_schema_stale_removes_pre_v4_db() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("stale.db");
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute_batch(
                "
                CREATE TABLE name_entries (
                    osm_id INTEGER PRIMARY KEY NOT NULL,
                    name TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    lat REAL NOT NULL,
                    lon REAL NOT NULL,
                    sub_area TEXT NOT NULL DEFAULT '',
                    municipality TEXT NOT NULL DEFAULT '',
                    region_id TEXT NOT NULL DEFAULT ''
                );
                PRAGMA user_version = 3;
                ",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO name_entries(osm_id, name, kind, lat, lon, region_id)
                 VALUES (1,'Named Hall','named',61.0,10.0,'europe/norway/ostlandet')",
                [],
            )
            .unwrap();
        }
        assert!(!NameIndex::is_current_schema(&db));
        assert!(
            !NameIndex::discard_if_schema_stale(&db),
            "must not delete the shared DB on schema mismatch"
        );
        assert!(db.is_file());
        assert!(!NameIndex::discard_if_schema_stale(&db));
    }

    #[test]
    fn recover_wal_sidecars_checkpoints_and_drops_shm() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("place_index.db");
        let wal = PathBuf::from(format!("{}-wal", db.display()));
        let shm = PathBuf::from(format!("{}-shm", db.display()));
        let wal_bak = dir.path().join("wal.bak");
        {
            let mut idx = NameIndex::open(&db).expect("open");
            // Keep frames in the WAL until we snapshot sidecars (close would
            // often auto-checkpoint a tiny DB).
            idx.conn
                .execute_batch("PRAGMA wal_autocheckpoint=0;")
                .unwrap();
            idx.upsert_entry_with_region(
                1,
                "Tunis Medina".into(),
                "place:suburb".into(),
                36.8,
                10.1,
                String::new(),
                String::new(),
                "africa/tunisia".into(),
            )
            .unwrap();
            assert!(wal.is_file(), "WAL should exist while writer is open");
            std::fs::copy(&wal, &wal_bak).expect("snapshot wal");
        }
        // Restore WAL + torn shm as after a crash (close may have truncated).
        std::fs::copy(&wal_bak, &wal).expect("restore wal");
        std::fs::write(&shm, [0u8; 64]).expect("torn shm");
        NameIndex::recover_wal_sidecars(&db).expect("recover");
        assert!(
            !shm.is_file() || std::fs::metadata(&shm).map(|m| m.len()).unwrap_or(0) != 64,
            "torn shm must be removed or replaced"
        );
        let reader = NameIndex::open_readonly(&db).expect("readonly after recover");
        let hits = reader.search("Tunis", 8).expect("search");
        assert!(
            hits.iter().any(|h| h.name.contains("Tunis")),
            "rows must survive recover, got {hits:?}"
        );
        assert!(
            NameIndex::has_entries_for_region(&db, "africa/tunisia"),
            "tunisia region rows intact"
        );
    }

    #[test]
    fn multi_region_index_is_additive_and_reindex_preserves_other() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("place_index.db");
        let mut idx = NameIndex::open(&db).expect("open");
        idx.upsert_entry_with_region(
            101,
            "Lillehammer".into(),
            "place:town".into(),
            61.11,
            10.46,
            String::new(),
            String::new(),
            "europe/norway/ostlandet".into(),
        )
        .unwrap();
        idx.upsert_entry_with_region(
            201,
            "Bergen".into(),
            "place:city".into(),
            60.39,
            5.32,
            String::new(),
            String::new(),
            "europe/norway/vestlandet".into(),
        )
        .unwrap();
        drop(idx);

        assert!(NameIndex::has_entries_for_region(
            &db,
            "europe/norway/ostlandet"
        ));
        assert!(NameIndex::has_entries_for_region(
            &db,
            "europe/norway/vestlandet"
        ));

        // Simulate re-index of region A: clear A then re-upsert.
        let mut idx = NameIndex::open(&db).expect("reopen");
        idx.clear_region("europe/norway/ostlandet").unwrap();
        idx.upsert_entry_with_region(
            102,
            "Gjøvik".into(),
            "place:town".into(),
            60.80,
            10.69,
            String::new(),
            String::new(),
            "europe/norway/ostlandet".into(),
        )
        .unwrap();

        let ost = idx.search("Gjøvik", 8).unwrap();
        assert!(ost.iter().any(|h| h.name == "Gjøvik"), "got {ost:?}");
        let vest = idx.search("Bergen", 8).unwrap();
        assert!(
            vest.iter().any(|h| h.name == "Bergen"),
            "vestlandet wiped: {vest:?}"
        );
        let old = idx.search("Lillehammer", 8).unwrap();
        assert!(
            old.iter().all(|h| h.name != "Lillehammer"),
            "old ostlandet row should be gone"
        );
    }

    #[test]
    fn has_entries_false_on_empty_and_true_after_upsert() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("empty.db");
        NameIndex::open(&db).expect("create");
        assert!(!NameIndex::has_entries(&db));
        let mut idx = NameIndex::open(&db).expect("reopen");
        idx.upsert_entry(1, "Oslo".into(), "place:city".into(), 59.91, 10.75)
            .unwrap();
        drop(idx);
        assert!(NameIndex::has_entries(&db));
    }

    #[test]
    fn has_entries_missing_file_does_not_create() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("no-such.db");
        assert!(!NameIndex::has_entries(&db));
        assert!(!db.exists());
    }

    /// Orphan FTS rows (content deleted without FTS delete) must not surface after
    /// clear_region: search JOINs to name_entries, and clear rebuilds FTS.
    #[test]
    fn clear_region_rebuild_drops_orphan_fts_hits() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("orphan.db");
        let mut idx = NameIndex::open(&db).expect("create");
        idx.upsert_entry_with_region(
            42,
            "GamleNavn".into(),
            "place:village".into(),
            60.0,
            10.0,
            String::new(),
            String::new(),
            "europe/norway/ostlandet".into(),
        )
        .unwrap();
        // Simulate Android clear without FTS5: wipe content, leave FTS stale.
        idx.conn
            .execute(
                "DELETE FROM name_entries WHERE region_id = ?1",
                ["europe/norway/ostlandet"],
            )
            .unwrap();
        // Stale FTS may still MATCH; JOIN should yield nothing.
        let pre = idx.search("GamleNavn", 8).unwrap();
        assert!(
            pre.iter().all(|h| h.name != "GamleNavn"),
            "JOIN must hide orphans before rebuild: {pre:?}"
        );
        // clear_region (with rebuild) then re-index under a new name.
        idx.clear_region("europe/norway/ostlandet").unwrap();
        idx.upsert_entry_with_region(
            42,
            "NyttNavn".into(),
            "place:village".into(),
            60.0,
            10.0,
            String::new(),
            String::new(),
            "europe/norway/ostlandet".into(),
        )
        .unwrap();
        let old = idx.search("GamleNavn", 8).unwrap();
        assert!(
            old.iter()
                .all(|h| h.name != "GamleNavn" && h.name != "NyttNavn"),
            "old name must not match after rebuild+rename: {old:?}"
        );
        let neu = idx.search("NyttNavn", 8).unwrap();
        assert!(neu.iter().any(|h| h.name == "NyttNavn"), "got {neu:?}");
    }

    #[test]
    fn classify_named_building_kind_and_amenity_override() {
        let building = classify_named(
            435718754,
            61.885_475,
            10.737_108,
            [("building", "yes"), ("name", "Espedalsvegen 656")].into_iter(),
        )
        .expect("named building");
        assert_eq!(building.3, NAMED_BUILDING_KIND);
        assert_eq!(building.1, "Espedalsvegen 656");

        let amenity = classify_named(
            1,
            60.0,
            10.0,
            [
                ("building", "yes"),
                ("name", "Rådhuset"),
                ("amenity", "townhall"),
            ]
            .into_iter(),
        )
        .expect("townhall");
        assert_eq!(amenity.3, "amenity:townhall");

        assert!(classify_named(
            2,
            60.0,
            10.0,
            [("building", "no"), ("name", "X")].into_iter()
        )
        .is_some());
        let not_building = classify_named(
            2,
            60.0,
            10.0,
            [("building", "no"), ("name", "X")].into_iter(),
        )
        .unwrap();
        assert_eq!(not_building.3, "named");
    }

    /// Espedalsvegen 656 area (OSM way/435718754 coords) round-trips via bbox query.
    #[test]
    fn named_buildings_in_bbox_round_trip_espedal() {
        let mut idx = NameIndex::open_in_memory().expect("mem");
        // Simulate classify_named + upsert for the reported building.
        let (id, name, _search_doc, kind, _names_json, lat, lon) = classify_named(
            435718754,
            61.885_475,
            10.737_108,
            [("building", "yes"), ("name", "Espedalsvegen 656")].into_iter(),
        )
        .unwrap();
        assert_eq!(kind, NAMED_BUILDING_KIND);
        idx.upsert_entry(id, name.clone(), kind, lat, lon).unwrap();
        // Distractors: city + generic named (not building).
        idx.upsert_entry(9, "Lillehammer".into(), "place:city".into(), 61.115, 10.466)
            .unwrap();
        idx.upsert_entry(10, "Some Peak Label".into(), "named".into(), 61.885, 10.737)
            .unwrap();

        let hits = idx
            .named_buildings_in_bbox(61.88, 10.73, 61.89, 10.74, 32)
            .unwrap();
        assert_eq!(hits.len(), 1, "expected only the building: {hits:?}");
        assert_eq!(hits[0].osm_id, 435718754);
        assert_eq!(hits[0].name, "Espedalsvegen 656");
        assert_eq!(hits[0].kind, NAMED_BUILDING_KIND);
        assert!((hits[0].lat - 61.885_475).abs() < 1e-6);
        assert!((hits[0].lon - 10.737_108).abs() < 1e-6);

        let empty = idx
            .named_buildings_in_bbox(59.0, 10.0, 60.0, 11.0, 8)
            .unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn file_index_uses_wal_and_synchronous_normal() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("place_index.db");
        let idx = NameIndex::open(&db).expect("open");
        let mode: String = idx
            .conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
        let sync: i64 = idx
            .conn
            .query_row("PRAGMA synchronous", [], |row| row.get(0))
            .unwrap();
        assert_eq!(sync, 1, "WAL should use synchronous=NORMAL (1), got {sync}");
    }

    #[test]
    fn wal_readonly_query_does_not_wait_on_writer_transaction() {
        use std::time::Instant;
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("wal.db");
        let mut writer = NameIndex::open(&db).expect("open");
        writer
            .upsert_entry_with_region(
                1,
                "Oslo".into(),
                "place:city".into(),
                59.91,
                10.75,
                String::new(),
                String::new(),
                "europe/norway/ostlandet".into(),
            )
            .unwrap();
        let tx = writer.conn.unchecked_transaction().unwrap();
        tx.execute(
            "UPDATE name_entries SET name = 'HIDDEN' WHERE osm_id = 1",
            [],
        )
        .unwrap();
        let t0 = Instant::now();
        let reader = NameIndex::open_readonly(&db).expect("readonly");
        let hits = reader.search("Oslo", 8).unwrap();
        let elapsed = t0.elapsed();
        assert!(
            elapsed.as_millis() < 200,
            "readonly open/search blocked for {elapsed:?} (expected WAL snapshot)"
        );
        assert!(
            hits.iter().any(|h| h.name == "Oslo"),
            "WAL reader should see committed snapshot, got {hits:?}"
        );
        drop(hits);
        drop(reader);
        tx.rollback().unwrap();
    }

    #[test]
    fn incomplete_build_is_not_region_index_complete() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("partial.db");
        let mut idx = NameIndex::open(&db).expect("open");
        idx.upsert_entry_with_region(
            1,
            "Oslo".into(),
            "place:city".into(),
            59.91,
            10.75,
            String::new(),
            String::new(),
            "europe/norway/ostlandet".into(),
        )
        .unwrap();
        idx.conn
            .execute(
                "INSERT INTO name_index_build(region_id, expected, written, complete)
                 VALUES ('europe/norway/ostlandet', 100, 1, 0)
                 ON CONFLICT(region_id) DO UPDATE SET written=1, complete=0",
                [],
            )
            .unwrap();
        drop(idx);
        assert!(NameIndex::has_entries_for_region(
            &db,
            "europe/norway/ostlandet"
        ));
        assert!(
            !NameIndex::region_index_complete(&db, "europe/norway/ostlandet"),
            "partial write must not cache-hit"
        );
    }

    #[test]
    fn place_index_build_lock_serializes_concurrent_callers() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::thread;
        use std::time::Duration;

        static INSIDE: AtomicUsize = AtomicUsize::new(0);
        static MAX: AtomicUsize = AtomicUsize::new(0);

        let threads: Vec<_> = (0..4)
            .map(|_| {
                thread::spawn(|| {
                    let _g = lock_place_index_build();
                    let now = INSIDE.fetch_add(1, Ordering::SeqCst) + 1;
                    MAX.fetch_max(now, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(30));
                    INSIDE.fetch_sub(1, Ordering::SeqCst);
                })
            })
            .collect();
        for t in threads {
            t.join().expect("thread");
        }
        assert_eq!(
            MAX.load(Ordering::SeqCst),
            1,
            "two place-index builders must not overlap"
        );
    }

    #[test]
    fn place_index_build_lock_wait_sets_progress_label() {
        use std::sync::{Arc, Barrier};
        use std::thread;
        use std::time::Duration;

        let _prog = crate::download::progress::test_lock();
        crate::download::progress::clear();
        let hold = Arc::new(Barrier::new(2));
        let released = Arc::new(Barrier::new(2));
        let holder = thread::spawn({
            let hold = hold.clone();
            let released = released.clone();
            move || {
                let _g = lock_place_index_build();
                hold.wait();
                released.wait();
            }
        });
        hold.wait();
        let waiter = thread::spawn(|| {
            let _g = lock_place_index_build_with_progress();
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            let snap = crate::download::progress::snapshot();
            if snap.label == "Place index: waiting for another index build…" {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "wait label never appeared, last={:?}",
                snap.label
            );
            thread::sleep(Duration::from_millis(5));
        }
        let snap = crate::download::progress::snapshot();
        assert_eq!(snap.units_done, 0);
        assert_eq!(snap.units_total, Some(6));
        released.wait();
        waiter.join().expect("waiter");
        holder.join().expect("holder");
    }

    /// Task 4 race: ensure_place_index (build lock + WAL open) concurrent with
    /// clear on the same DB. Pre-fix Android SQLite clear without this lock
    /// SIGBUS'd ~1/4 of windows; under the shared lock every iteration must
    /// keep max concurrent holders at 1 and finish without error.
    #[test]
    fn clear_region_rows_serializes_against_build_lock_stress() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Barrier};
        use std::thread;
        use std::time::Duration;

        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("place_index.db");
        {
            let mut idx = NameIndex::open(&db).expect("open");
            idx.upsert_entry_with_region(
                1,
                "Alpha".into(),
                "place".into(),
                60.0,
                10.0,
                String::new(),
                String::new(),
                "europe/norway/ostlandet".into(),
            )
            .expect("upsert a");
            idx.upsert_entry_with_region(
                2,
                "Beta".into(),
                "place".into(),
                57.0,
                12.0,
                String::new(),
                String::new(),
                "europe/sweden/halland".into(),
            )
            .expect("upsert b");
        }

        let concurrent = Arc::new(AtomicUsize::new(0));
        let max_concurrent = Arc::new(AtomicUsize::new(0));
        let failures = Arc::new(AtomicUsize::new(0));
        const ITERS: usize = 100;

        for i in 0..ITERS {
            let barrier = Arc::new(Barrier::new(2));
            let db_a = db.clone();
            let db_b = db.clone();
            let concurrent_a = concurrent.clone();
            let max_a = max_concurrent.clone();
            let concurrent_b = concurrent.clone();
            let max_b = max_concurrent.clone();
            let failures_b = failures.clone();
            let region = if i % 2 == 0 {
                "europe/sweden/halland"
            } else {
                "europe/norway/ostlandet"
            };

            let builder = thread::spawn({
                let barrier = barrier.clone();
                move || {
                    // Mimic ensure_place_index: hold build lock + open WAL.
                    let _g = lock_place_index_build();
                    let now = concurrent_a.fetch_add(1, Ordering::SeqCst) + 1;
                    max_a.fetch_max(now, Ordering::SeqCst);
                    barrier.wait();
                    let _idx = NameIndex::open(&db_a).expect("builder open");
                    thread::sleep(Duration::from_millis(5));
                    concurrent_a.fetch_sub(1, Ordering::SeqCst);
                }
            });
            let clearer = thread::spawn({
                let barrier = barrier.clone();
                move || {
                    barrier.wait();
                    // Same lock + rusqlite stack as clear_place_index_region_rows
                    // (and as Kotlin's preferred FFI path). Counting under the
                    // lock proves we never overlap the builder's WAL open.
                    let _g = lock_place_index_build();
                    let now = concurrent_b.fetch_add(1, Ordering::SeqCst) + 1;
                    max_b.fetch_max(now, Ordering::SeqCst);
                    let clear_result = (|| {
                        let mut idx = NameIndex::open(&db_b).map_err(|e| e.to_string())?;
                        idx.clear_region(region).map_err(|e| e.to_string())?;
                        Ok::<(), String>(())
                    })();
                    concurrent_b.fetch_sub(1, Ordering::SeqCst);
                    drop(_g);
                    if let Err(e) = clear_result {
                        failures_b.fetch_add(1, Ordering::SeqCst);
                        eprintln!("clear failed: {e}");
                    }
                }
            });
            builder.join().expect("builder");
            clearer.join().expect("clearer");
        }

        assert_eq!(
            failures.load(Ordering::SeqCst),
            0,
            "clear must not fail (was: database is locked / SIGBUS)"
        );
        assert_eq!(
            max_concurrent.load(Ordering::SeqCst),
            1,
            "build lock must serialize builder and clearer ({ITERS} iters)"
        );
        // Re-open after the race window: WAL must still be readable (no
        // truncated -shm / SIGBUS-class corruption).
        NameIndex::open(&db).expect("reopen after stress");
    }

    #[test]
    fn clear_place_index_region_rows_api_waits_on_build_lock() {
        use std::sync::{Arc, Barrier};
        use std::thread;
        use std::time::{Duration, Instant};

        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("place_index.db");
        {
            let mut idx = NameIndex::open(&db).expect("open");
            idx.upsert_entry_with_region(
                1,
                "X".into(),
                "place".into(),
                60.0,
                10.0,
                String::new(),
                String::new(),
                "europe/norway/ostlandet".into(),
            )
            .expect("upsert");
        }

        let hold = Arc::new(Barrier::new(2));
        let released = Arc::new(Barrier::new(2));
        let db_hold = db.clone();
        let holder = thread::spawn({
            let hold = hold.clone();
            let released = released.clone();
            move || {
                let _g = lock_place_index_build();
                let _idx = NameIndex::open(&db_hold).expect("hold open");
                hold.wait();
                thread::sleep(Duration::from_millis(50));
                released.wait();
            }
        });
        hold.wait();
        let started = Instant::now();
        let clearer = thread::spawn({
            let db = db.clone();
            move || clear_place_index_region_rows(&db, "europe/norway/ostlandet")
        });
        thread::sleep(Duration::from_millis(20));
        assert!(
            !clearer.is_finished(),
            "clear_place_index_region_rows must block while build lock is held"
        );
        released.wait();
        let result = clearer.join().expect("clearer");
        holder.join().expect("holder");
        assert!(result.is_ok(), "clear after release: {result:?}");
        assert!(
            started.elapsed() >= Duration::from_millis(50),
            "clear returned before holder finished"
        );
    }

    #[test]
    fn ffi_acquire_release_blocks_concurrent_builder() {
        use std::sync::{Arc, Barrier};
        use std::thread;
        use std::time::{Duration, Instant};

        let hold = Arc::new(Barrier::new(2));
        let released = Arc::new(Barrier::new(2));
        let acquirer = thread::spawn({
            let hold = hold.clone();
            let released = released.clone();
            move || {
                place_index_build_lock_acquire().expect("acquire");
                assert!(place_index_build_lock_held_on_thread());
                hold.wait();
                released.wait();
                place_index_build_lock_release().expect("release");
            }
        });
        hold.wait();
        let started = Instant::now();
        let builder = thread::spawn(|| {
            let _g = lock_place_index_build();
        });
        thread::sleep(Duration::from_millis(40));
        assert!(
            !builder.is_finished(),
            "builder must block while FFI acquire holds the lock"
        );
        released.wait();
        builder.join().expect("builder");
        acquirer.join().expect("acquirer");
        assert!(
            started.elapsed() >= Duration::from_millis(40),
            "builder returned too fast"
        );
    }

    /// Synthetic multi-region DB used to decide whether `backfill_legacy_complete`
    /// dominates `NameIndex::open`. Ignored: seeds >= 2M rows (slow, disk-heavy).
    #[test]
    #[ignore]
    fn open_two_million_rows_prints_timings() {
        use std::time::Instant;

        struct PhaseLog;
        impl log::Log for PhaseLog {
            fn enabled(&self, metadata: &log::Metadata) -> bool {
                metadata.target() == "PHASE_TIMING"
            }
            fn log(&self, record: &log::Record) {
                if self.enabled(record.metadata()) {
                    println!("{}", record.args());
                }
            }
            fn flush(&self) {}
        }
        static LOGGER: PhaseLog = PhaseLog;
        let _ = log::set_logger(&LOGGER);
        log::set_max_level(log::LevelFilter::Info);

        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("big.db");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(
                "
                PRAGMA journal_mode = WAL;
                PRAGMA synchronous = OFF;
                CREATE TABLE name_entries (
                    osm_id INTEGER PRIMARY KEY NOT NULL,
                    name TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    lat REAL NOT NULL,
                    lon REAL NOT NULL,
                    sub_area TEXT NOT NULL DEFAULT '',
                    municipality TEXT NOT NULL DEFAULT '',
                    region_id TEXT NOT NULL DEFAULT ''
                );
                CREATE TABLE name_index_build (
                    region_id TEXT PRIMARY KEY NOT NULL,
                    expected INTEGER NOT NULL DEFAULT 0,
                    written INTEGER NOT NULL DEFAULT 0,
                    complete INTEGER NOT NULL DEFAULT 0
                );
                ",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO name_entries(osm_id, name, kind, lat, lon, region_id)
                 VALUES (1, 'n', 'place:village', 60.0, 10.0, 'europe/norway/ostlandet')",
                [],
            )
            .unwrap();
            loop {
                let n: i64 = conn
                    .query_row("SELECT COUNT(*) FROM name_entries", [], |row| row.get(0))
                    .unwrap();
                if n >= 2_000_000 {
                    break;
                }
                conn.execute(
                    "INSERT INTO name_entries(osm_id, name, kind, lat, lon, sub_area, municipality, region_id)
                     SELECT osm_id + ?1, name, kind, lat, lon, sub_area, municipality,
                       CASE ((osm_id + ?1) % 3)
                         WHEN 0 THEN 'europe/norway/ostlandet'
                         WHEN 1 THEN 'europe/norway/vestlandet'
                         ELSE 'europe/norway/trondelag'
                       END
                     FROM name_entries",
                    params![n],
                )
                .unwrap();
            }
            let n: i64 = conn
                .query_row("SELECT COUNT(*) FROM name_entries", [], |row| row.get(0))
                .unwrap();
            let distinct: i64 = conn
                .query_row(
                    "SELECT COUNT(DISTINCT region_id) FROM name_entries",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            println!("seeded {n} name_entries rows across {distinct} region ids");
            conn.execute_batch(
                "
                INSERT INTO name_index_build(region_id, expected, written, complete)
                SELECT region_id, COUNT(*), COUNT(*), 1 FROM name_entries GROUP BY region_id;
                ",
            )
            .unwrap();
        }

        {
            let conn = Connection::open(&db).unwrap();
            conn.busy_timeout(Duration::from_secs(5)).unwrap();
            let t0 = Instant::now();
            NameIndex::apply_file_pragmas(&conn).unwrap();
            println!("apply_file_pragmas {:?}", t0.elapsed());
            let t0 = Instant::now();
            NameIndex::migrate(&conn).unwrap();
            println!("migrate {:?}", t0.elapsed());
            let t0 = Instant::now();
            NameIndex::backfill_legacy_complete(&conn).unwrap();
            println!("backfill_legacy_complete {:?}", t0.elapsed());
        }

        let t0 = Instant::now();
        let _idx = NameIndex::open(&db).expect("open 2M-row index");
        println!(
            "NameIndex::open total {:?} (backfill skipped when name_index_build already exists)",
            t0.elapsed()
        );
    }

    fn name_entries_ddl_with_region() -> &'static str {
        "
            CREATE TABLE name_entries (
                osm_id INTEGER PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                lat REAL NOT NULL,
                lon REAL NOT NULL,
                sub_area TEXT NOT NULL DEFAULT '',
                municipality TEXT NOT NULL DEFAULT '',
                region_id TEXT NOT NULL DEFAULT ''
            );
        "
    }

    #[test]
    fn open_backfills_legacy_db_without_build_table() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("legacy-no-build.db");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(name_entries_ddl_with_region()).unwrap();
            conn.execute(
                "INSERT INTO name_entries(osm_id, name, kind, lat, lon, region_id)
                 VALUES (1, 'Oslo', 'place:city', 59.91, 10.75, 'europe/norway/ostlandet')",
                [],
            )
            .unwrap();
        }
        let _idx = NameIndex::open(&db).expect("open legacy");
        let conn = Connection::open(&db).unwrap();
        let complete: i64 = conn
            .query_row(
                "SELECT complete FROM name_index_build WHERE region_id = 'europe/norway/ostlandet'",
                [],
                |row| row.get(0),
            )
            .expect("name_index_build row");
        assert_eq!(complete, 1);
    }

    #[test]
    fn open_skips_backfill_when_build_table_already_exists() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("has-build.db");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(name_entries_ddl_with_region()).unwrap();
            conn.execute_batch(
                "
                CREATE TABLE name_index_build (
                    region_id TEXT PRIMARY KEY NOT NULL,
                    expected INTEGER NOT NULL DEFAULT 0,
                    written INTEGER NOT NULL DEFAULT 0,
                    complete INTEGER NOT NULL DEFAULT 0
                );
                ",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO name_entries(osm_id, name, kind, lat, lon, region_id)
                 VALUES (1, 'Oslo', 'place:city', 59.91, 10.75, 'europe/norway/ostlandet')",
                [],
            )
            .unwrap();
        }
        let _idx = NameIndex::open(&db).expect("open");
        let conn = Connection::open(&db).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM name_index_build", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(
            n, 0,
            "GROUP BY backfill must not run when name_index_build already existed"
        );
    }

    #[test]
    fn plan_during_index_pauses_and_resumes_same_rows() {
        use crate::download::pbf_priority::{self, ForegroundPlanGuard};
        let _g = pbf_priority::lock_plan_flag_for_test();
        let pbf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/place-source-tiny.osm.pbf");
        assert!(pbf.is_file(), "{}", pbf.display());
        let dir = tempfile::tempdir().expect("tmpdir");
        let uninterrupted = dir.path().join("full.db");
        let paused = dir.path().join("paused.db");
        let region = "test/fu38-pause";
        let n_full = {
            let mut idx = NameIndex::open(&uninterrupted).unwrap();
            idx.load_from_pbf_for_region(&pbf, region).unwrap()
        };
        assert!(n_full > 0);
        {
            let _fg = ForegroundPlanGuard::acquire();
            let mut idx = NameIndex::open(&paused).unwrap();
            let err = idx
                .load_from_pbf_for_region(&pbf, region)
                .expect_err("must pause");
            assert!(
                err.to_string().contains(PLACE_INDEX_PAUSED_PREFIX),
                "{err:#}"
            );
            assert!(
                !NameIndex::region_index_intact(&paused, region),
                "paused build must not look intact"
            );
        }
        let n_resume = {
            let mut idx = NameIndex::open(&paused).unwrap();
            idx.load_from_pbf_for_region(&pbf, region).unwrap()
        };
        assert_eq!(n_resume, n_full);
        assert!(NameIndex::region_index_intact(&paused, region));
        let full_ids =
            NameIndex::osm_ids_for_region(&Connection::open(&uninterrupted).unwrap(), region)
                .unwrap();
        let resume_ids =
            NameIndex::osm_ids_for_region(&Connection::open(&paused).unwrap(), region).unwrap();
        assert_eq!(full_ids, resume_ids);
    }

    #[test]
    fn search_ranks_exact_local_place_before_foreign_prefix() {
        let mut idx = NameIndex::open_in_memory().unwrap();
        idx.upsert_entry_with_region(
            1,
            "Aga".into(),
            "place:hamlet".into(),
            60.30,
            6.60,
            String::new(),
            "Ullensvang".into(),
            "europe/norway/vestlandet".into(),
        )
        .unwrap();
        idx.upsert_entry_with_region(
            2,
            "Agardh".into(),
            "place:hamlet".into(),
            56.67,
            12.86,
            String::new(),
            String::new(),
            "europe/sweden/halland".into(),
        )
        .unwrap();
        let hits = idx
            .search_biased(
                "Aga",
                10,
                Some(60.39),
                Some(6.50),
                "europe/norway/vestlandet",
            )
            .unwrap();
        assert_eq!(hits[0].name, "Aga");
        assert_eq!(hits[0].region_id, "europe/norway/vestlandet");
    }

    #[test]
    fn exact_as_typed_name_beats_folded_name_of_a_larger_kind() {
        let mut idx = NameIndex::open_in_memory().unwrap();
        idx.upsert_entry_with_region(
            1,
            "Åre".into(),
            "place:farm".into(),
            59.37,
            10.18,
            String::new(),
            "Våje".into(),
            "europe/norway/ostlandet".into(),
        )
        .unwrap();
        idx.upsert_entry_with_region(
            2,
            "Are".into(),
            "place:hamlet".into(),
            59.36,
            5.36,
            String::new(),
            "Bokn".into(),
            "europe/norway/vestlandet".into(),
        )
        .unwrap();
        let names = |q: &str| -> Vec<String> {
            idx.search_biased(q, 10, Some(63.43), Some(10.40), "europe/norway/trondelag")
                .unwrap()
                .into_iter()
                .map(|h| h.name)
                .collect()
        };
        assert_eq!(names("Åre"), vec!["Åre", "Are"]);
        assert_eq!(names("åre"), vec!["Åre", "Are"]);
        assert_eq!(
            names("Are"),
            vec!["Are", "Åre"],
            "plain query: hamlet before farm"
        );
        assert_eq!(search_match_class("Åre", "Åre"), 0);
        assert_eq!(search_match_class("Are", "Åre"), 1);
        assert_eq!(search_match_class("Åre", "Are"), 1);
        assert_eq!(search_match_class("Are", "Are"), 1);
        assert_eq!(search_match_class("Årestua", "Åre"), 2);
    }

    #[test]
    fn search_keeps_prefix_only_hits() {
        let mut idx = NameIndex::open_in_memory().unwrap();
        idx.upsert_entry_with_region(
            1,
            "Raufoss".into(),
            "place:town".into(),
            60.73,
            10.61,
            String::new(),
            "Vestre Toten".into(),
            "europe/norway/ostlandet".into(),
        )
        .unwrap();
        let hits = idx.search("Raufo", 10).unwrap();
        assert!(hits.iter().any(|h| h.name == "Raufoss"));
    }

    #[test]
    fn pause_request_stops_each_index_phase_within_two_seconds() {
        use crate::download::pbf_priority::{self, ForegroundPlanGuard};
        let _g = pbf_priority::lock_plan_flag_for_test();
        pbf_priority::TEST_INDEX_PHASE_SLEEP_MS.store(400, std::sync::atomic::Ordering::Relaxed);
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("v4.db");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(
                "
                CREATE TABLE name_entries (
                    osm_id INTEGER PRIMARY KEY NOT NULL,
                    name TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    lat REAL NOT NULL,
                    lon REAL NOT NULL,
                    sub_area TEXT NOT NULL DEFAULT '',
                    municipality TEXT NOT NULL DEFAULT '',
                    region_id TEXT NOT NULL DEFAULT '',
                    search_doc TEXT NOT NULL DEFAULT ''
                );
                PRAGMA user_version = 4;
                ",
            )
            .unwrap();
            let tx = conn.unchecked_transaction().unwrap();
            for i in 0..12_000 {
                tx.execute(
                    "INSERT INTO name_entries(osm_id, name, kind, lat, lon, region_id, search_doc)
                     VALUES (?1, ?2, 'place:hamlet', 60.0, 10.0, 'europe/norway/ostlandet', ?2)",
                    params![i, format!("n{i}")],
                )
                .unwrap();
            }
            tx.commit().unwrap();
        }
        let started = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let started2 = started.clone();
        let db2 = db.clone();
        let worker = std::thread::spawn(move || {
            pbf_priority::honour_idle_pause_on_this_thread();
            started2.store(true, std::sync::atomic::Ordering::Relaxed);
            NameIndex::open(&db2)
        });
        while !started.load(std::sync::atomic::Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        std::thread::sleep(std::time::Duration::from_millis(80));
        let t0 = std::time::Instant::now();
        pbf_priority::request_idle_pause();
        let _fg = ForegroundPlanGuard::acquire();
        let err = match worker.join().expect("join") {
            Ok(_) => panic!("migrate must pause"),
            Err(e) => e,
        };
        pbf_priority::TEST_INDEX_PHASE_SLEEP_MS.store(0, std::sync::atomic::Ordering::Relaxed);
        pbf_priority::clear_idle_pause();
        let waited = t0.elapsed();
        assert!(err.to_string().contains(PLACE_INDEX_PAUSED_PREFIX), "{err}");
        assert!(
            waited.as_millis() <= 2000,
            "plan waited {waited:?} for migrate pause"
        );
    }

    #[test]
    fn pause_request_honoured_in_every_index_phase() {
        use crate::download::pbf_priority::{self, ForegroundPlanGuard};
        let _g = pbf_priority::lock_plan_flag_for_test();
        let phases = [
            IndexBuildPhase::ReadSource,
            IndexBuildPhase::SortPrepare,
            IndexBuildPhase::Insert,
            IndexBuildPhase::Fts,
            IndexBuildPhase::Commit,
            IndexBuildPhase::Migrate,
        ];
        for phase in phases {
            pbf_priority::clear_idle_pause();
            pbf_priority::TEST_INDEX_PHASE_SLEEP_MS
                .store(300, std::sync::atomic::Ordering::Relaxed);
            let worker = std::thread::spawn(move || {
                pbf_priority::honour_idle_pause_on_this_thread();
                loop {
                    index_phase_tick(phase);
                    if pause_index_if_plan_active(0, 0, 0).is_err() {
                        break;
                    }
                }
            });
            std::thread::sleep(std::time::Duration::from_millis(40));
            let t0 = std::time::Instant::now();
            pbf_priority::request_idle_pause();
            let _fg = ForegroundPlanGuard::acquire();
            worker.join().expect("phase worker");
            pbf_priority::TEST_INDEX_PHASE_SLEEP_MS.store(0, std::sync::atomic::Ordering::Relaxed);
            pbf_priority::clear_idle_pause();
            let waited = t0.elapsed();
            assert!(
                waited.as_millis() <= 2000,
                "phase {phase:?} waited {waited:?}"
            );
        }
    }

    #[test]
    fn migrate_resumes_after_repeated_pauses() {
        use crate::download::pbf_priority::{self, ForegroundPlanGuard};
        let _g = pbf_priority::lock_plan_flag_for_test();
        pbf_priority::TEST_INDEX_PHASE_SLEEP_MS.store(200, std::sync::atomic::Ordering::Relaxed);
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("v4-resume.db");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(
                "
                CREATE TABLE name_entries (
                    osm_id INTEGER PRIMARY KEY NOT NULL,
                    name TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    lat REAL NOT NULL,
                    lon REAL NOT NULL,
                    sub_area TEXT NOT NULL DEFAULT '',
                    municipality TEXT NOT NULL DEFAULT '',
                    region_id TEXT NOT NULL DEFAULT '',
                    search_doc TEXT NOT NULL DEFAULT ''
                );
                PRAGMA user_version = 4;
                ",
            )
            .unwrap();
            let tx = conn.unchecked_transaction().unwrap();
            for i in 0..16_000 {
                tx.execute(
                    "INSERT INTO name_entries(osm_id, name, kind, lat, lon, region_id, search_doc)
                     VALUES (?1, ?2, 'place:hamlet', 60.0, 10.0, 'europe/norway/ostlandet', ?2)",
                    params![i, format!("n{i}")],
                )
                .unwrap();
            }
            tx.commit().unwrap();
        }
        for _ in 0..3 {
            pbf_priority::clear_idle_pause();
            let db2 = db.clone();
            let worker = std::thread::spawn(move || {
                pbf_priority::honour_idle_pause_on_this_thread();
                NameIndex::open(&db2)
            });
            std::thread::sleep(std::time::Duration::from_millis(80));
            pbf_priority::request_idle_pause();
            let _fg = ForegroundPlanGuard::acquire();
            let _ = worker.join().expect("join");
            drop(_fg);
            pbf_priority::clear_idle_pause();
        }
        pbf_priority::TEST_INDEX_PHASE_SLEEP_MS.store(0, std::sync::atomic::Ordering::Relaxed);
        pbf_priority::clear_idle_pause();
        let idx = NameIndex::open(&db).expect("resume must finish");
        drop(idx);
        let n: i64 = Connection::open(&db)
            .unwrap()
            .query_row("SELECT COUNT(*) FROM name_entries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 16_000);
        let v: i32 = Connection::open(&db)
            .unwrap()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, PLACE_INDEX_SCHEMA_VERSION);
    }
}
