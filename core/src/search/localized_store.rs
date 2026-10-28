//! Optional `names_json` column on place-index rows (localized OSM name tags).
//!
//! Additive only: `ALTER TABLE … ADD COLUMN` with empty default. Intact indexes
//! keep working; localized names appear on the next region build/update.

use std::collections::HashMap;
use std::sync::Mutex;

use rusqlite::{Connection, Result as SqlResult};
use serde_json::{Map, Value};

use crate::i18n::{localized_name_with_secondary, NameTags};

static PLACE_SEARCH_LOCALE: Mutex<String> = Mutex::new(String::new());

/// Device locale for place-search display (BCP 47). Empty → keep stored local `name`.
pub fn set_place_search_locale(locale: impl Into<String>) {
    if let Ok(mut g) = PLACE_SEARCH_LOCALE.lock() {
        *g = locale.into();
    }
}

pub fn place_search_locale() -> String {
    PLACE_SEARCH_LOCALE
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default()
}

pub fn ensure_names_json_column(conn: &Connection) -> SqlResult<()> {
    let mut cols: Vec<String> = Vec::new();
    let mut stmt = conn.prepare("PRAGMA table_info(name_entries)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for r in rows {
        cols.push(r?);
    }
    // Brand-new DB: migrate creates name_entries first; skip ALTER until then.
    if cols.is_empty() {
        return Ok(());
    }
    if !cols.iter().any(|c| c == "names_json") {
        conn.execute(
            "ALTER TABLE name_entries ADD COLUMN names_json TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }
    Ok(())
}

/// Compact JSON object of localization tags (`name:en`, `int_name`, …).
pub fn names_json_from_tags<'a>(tags: impl Iterator<Item = (&'a str, &'a str)>) -> String {
    let mut map = Map::new();
    for (k, v) in tags {
        let t = v.trim();
        if t.is_empty() {
            continue;
        }
        if k == "int_name" || (k.starts_with("name:") && !k.starts_with("name:etymology")) {
            map.insert(k.to_string(), Value::String(t.to_string()));
        }
    }
    if map.is_empty() {
        String::new()
    } else {
        Value::Object(map).to_string()
    }
}

/// Build owned tag map for selection (local name + JSON extras).
pub fn owned_name_tags(local_name: &str, names_json: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if !local_name.trim().is_empty() {
        out.insert("name".into(), local_name.to_string());
    }
    if names_json.is_empty() {
        return out;
    }
    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(names_json) {
        for (k, v) in map {
            if let Value::String(s) = v {
                if !s.trim().is_empty() {
                    out.insert(k, s);
                }
            }
        }
    }
    out
}

/// Localized primary, with optional ` · local` secondary when they differ.
pub fn display_name_for_locale(local_name: &str, names_json: &str) -> String {
    display_name_for_locale_tag(local_name, names_json, &place_search_locale())
}

pub fn display_name_for_locale_tag(local_name: &str, names_json: &str, locale: &str) -> String {
    if locale.is_empty() || names_json.is_empty() {
        return local_name.to_string();
    }
    let owned = owned_name_tags(local_name, names_json);
    let tags: NameTags<'_> = owned
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    match localized_name_with_secondary(&tags, locale) {
        Some(sel) => {
            if let Some(sec) = sel.secondary_local {
                if sec != sel.primary {
                    format!("{} · {}", sel.primary, sec)
                } else {
                    sel.primary
                }
            } else {
                sel.primary
            }
        }
        None => local_name.to_string(),
    }
}

/// Extra FTS tokens from localized name values (not keys).
pub fn localized_search_tokens(names_json: &str) -> Vec<String> {
    if names_json.is_empty() {
        return Vec::new();
    }
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(names_json) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (_k, v) in map {
        if let Value::String(s) = v {
            for part in s.split(';') {
                let t = part.trim();
                if !t.is_empty() {
                    out.push(t.to_string());
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_json_round_trip_and_display() {
        let json = names_json_from_tags(
            [
                ("name", "Oslo S"),
                ("name:en", "Oslo Central Station"),
                ("name:fr", "Gare centrale d'Oslo"),
            ]
            .into_iter(),
        );
        assert!(json.contains("name:en"));
        let disp = display_name_for_locale_tag("Oslo S", &json, "en");
        assert!(disp.starts_with("Oslo Central Station"), "{disp}");
        assert!(disp.contains("Oslo S"), "{disp}");
        let fr = display_name_for_locale_tag("Oslo S", &json, "fr");
        assert!(fr.contains("Gare centrale"), "{fr}");
    }

    #[test]
    fn empty_json_keeps_local() {
        assert_eq!(display_name_for_locale_tag("Espa", "", "en"), "Espa");
    }
}
