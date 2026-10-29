//! Shared OSM name selection for UI / guidance / search presentation.
//!
//! Order (see `docs/i18n/names.md`):
//! 1. `name:<full device locale>` (e.g. `nb-NO`)
//! 2. `name:<device language>` (`nb`, then `no` for Norwegian)
//! 3. If device language is Latin script and local `name` is not:
//!    `int_name`, then `name:en`, then `name:latin`, then other Latin-script
//!    `name:xx` in [`LATIN_NAME_FALLBACK_LANGS`] order (`name:fr` first)
//! 4. `name`
//!
//! Refs are never selected here.

use std::collections::HashMap;

/// Tag bag: keys like `name`, `name:en`, `int_name` (no `name:` prefix required for base).
pub type NameTags<'a> = HashMap<&'a str, &'a str>;

/// Result of name selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedName {
    /// Localized (or best-effort) primary label for the device locale.
    pub primary: String,
    /// Local `name` when it differs from [Self::primary] (for dual-line UI).
    pub secondary_local: Option<String>,
}

fn trim_nonempty(s: &str) -> Option<&str> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

fn get_tag<'a>(tags: &NameTags<'a>, key: &str) -> Option<&'a str> {
    tags.get(key).copied().and_then(trim_nonempty)
}

/// True when a majority of letters are Latin-script (Basic Latin + Latin-1 Supplement
/// + Latin Extended). Digits, spaces, and punctuation are ignored.
pub fn is_mostly_latin_script(s: &str) -> bool {
    let mut letters = 0u32;
    let mut latin = 0u32;
    for c in s.chars() {
        if !c.is_alphabetic() {
            continue;
        }
        letters += 1;
        if is_latin_letter(c) {
            latin += 1;
        }
    }
    if letters == 0 {
        // No letters (refs, numbers): treat as Latin-compatible for fallback purposes.
        return true;
    }
    latin * 2 >= letters
}

fn is_latin_letter(c: char) -> bool {
    matches!(
        c,
        'A'..='Z'
            | 'a'..='z'
            | '\u{00C0}'..='\u{024F}' // Latin-1 Supplement + Latin Extended-A/B start
            | '\u{1E00}'..='\u{1EFF}' // Latin Extended Additional
    )
}

/// Language codes that use a Latin-script orthography for UI fallback decisions.
fn language_is_latin_script(lang: &str) -> bool {
    let l = lang.to_ascii_lowercase();
    // Non-Latin UI languages we care about for the Latin-fallback branch.
    !matches!(
        l.as_str(),
        "ar" | "fa"
            | "ur"
            | "he"
            | "yi"
            | "hi"
            | "mr"
            | "ne"
            | "bn"
            | "pa"
            | "gu"
            | "ta"
            | "te"
            | "kn"
            | "ml"
            | "si"
            | "th"
            | "lo"
            | "my"
            | "ka"
            | "hy"
            | "am"
            | "ti"
            | "zh"
            | "ja"
            | "ko"
            | "ru"
            | "uk"
            | "be"
            | "bg"
            | "sr"
            | "mk"
            | "el"
    )
}

fn locale_parts(locale: &str) -> (String, String) {
    let raw = locale.trim().replace('_', "-");
    if raw.is_empty() {
        return ("en".into(), "en".into());
    }
    let lower = raw.to_ascii_lowercase();
    let lang = lower.split('-').next().unwrap_or("en").to_string();
    // Preserve region case for tag keys: OSM uses name:nb, name:en; full tags rare.
    let full = lower;
    (full, lang)
}

fn norwegian_language_fallbacks(lang: &str) -> Vec<String> {
    match lang {
        "nb" | "nn" => vec![lang.to_string(), "no".into()],
        "no" => vec!["no".into(), "nb".into()],
        _ => vec![lang.to_string()],
    }
}

fn pick_from_tags<'a>(tags: &NameTags<'a>, keys: &[&str]) -> Option<&'a str> {
    for k in keys {
        if let Some(v) = get_tag(tags, k) {
            return Some(v);
        }
    }
    None
}

/// Fixed preference among Latin-script `name:xx` when the device UI is Latin
/// and `int_name` / `name:en` / `name:latin` are absent. `fr` first (common in
/// Maghreb / bilingual OSM); then a stable Western/Central/Nordic/… list.
/// Keep in sync with `BasemapLocalizedLabels.LATIN_NAME_FALLBACK_LANGS`.
pub const LATIN_NAME_FALLBACK_LANGS: &[&str] = &[
    "fr", "de", "es", "it", "pt", "nl", "ca", "pl", "cs", "sk", "hu", "ro", "hr", "sl", "bs", "sq",
    "sv", "da", "nb", "nn", "no", "fi", "et", "lv", "lt", "tr", "id", "ms", "sw", "vi", "af", "eu",
    "gl", "cy", "ga", "is", "mt", "lb", "br", "fy",
];

/// Pick the first Latin-script `name:xx` for langs in [`LATIN_NAME_FALLBACK_LANGS`].
fn pick_latin_name_xx<'a>(tags: &NameTags<'a>) -> Option<&'a str> {
    for lang in LATIN_NAME_FALLBACK_LANGS {
        let key = format!("name:{lang}");
        if let Some(v) = get_tag(tags, &key) {
            if is_mostly_latin_script(v) {
                return Some(v);
            }
        }
    }
    None
}

/// Select primary display name for `locale` (BCP 47, e.g. `nb-NO`, `en`, `ar`).
pub fn localized_name(tags: &NameTags<'_>, locale: &str) -> Option<String> {
    localized_name_with_secondary(tags, locale).map(|s| s.primary)
}

/// Primary + optional local `name` when different (search / dual label).
pub fn localized_name_with_secondary(tags: &NameTags<'_>, locale: &str) -> Option<SelectedName> {
    let (full, lang) = locale_parts(locale);
    let local_name = get_tag(tags, "name").map(|s| s.to_string());

    let mut candidates: Vec<String> = Vec::new();

    // 1. name:<full locale>
    candidates.push(format!("name:{full}"));
    // Also try original casing variants for region (nb-no already lowercased).
    if full.contains('-') {
        let parts: Vec<_> = full.split('-').collect();
        if parts.len() >= 2 {
            candidates.push(format!(
                "name:{}-{}",
                parts[0],
                parts[1].to_ascii_uppercase()
            ));
        }
    }

    // 2. name:<lang> (+ Norwegian no)
    for l in norwegian_language_fallbacks(&lang) {
        candidates.push(format!("name:{l}"));
    }

    let mut primary = pick_from_tags(
        tags,
        &candidates.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
    )
    .map(|s| s.to_string());

    // 3. Latin-script device language + non-Latin local name →
    //    int_name / name:en / name:latin / other Latin name:xx (fr first)
    if primary.is_none() && language_is_latin_script(&lang) {
        let local_non_latin = local_name
            .as_deref()
            .is_some_and(|n| !is_mostly_latin_script(n));
        if local_non_latin || local_name.is_none() {
            primary = pick_from_tags(tags, &["int_name", "name:en", "name:latin"])
                .or_else(|| pick_latin_name_xx(tags))
                .map(|s| s.to_string());
        }
    }

    // 4. name
    if primary.is_none() {
        primary = local_name.clone();
    }

    let primary = primary?;
    let secondary_local = local_name.filter(|n| n != &primary);
    Some(SelectedName {
        primary,
        secondary_local,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags<'a>(pairs: &'a [(&'a str, &'a str)]) -> NameTags<'a> {
        let mut m = NameTags::new();
        for (k, v) in pairs {
            m.insert(*k, *v);
        }
        m
    }

    #[test]
    fn prefers_full_locale_then_lang() {
        let t = tags(&[
            ("name", "Oslo"),
            ("name:nb-NO", "Oslo NB-NO"),
            ("name:nb", "Oslo NB"),
        ]);
        assert_eq!(localized_name(&t, "nb-NO").as_deref(), Some("Oslo NB-NO"));
        let t2 = tags(&[("name", "Oslo"), ("name:nb", "Oslo NB")]);
        assert_eq!(localized_name(&t2, "nb-NO").as_deref(), Some("Oslo NB"));
    }

    #[test]
    fn norwegian_falls_back_to_no() {
        let t = tags(&[("name", "Bergen"), ("name:no", "Bergen NO")]);
        assert_eq!(localized_name(&t, "nb").as_deref(), Some("Bergen NO"));
    }

    #[test]
    fn latin_ui_gets_int_name_when_local_is_arabic() {
        // German has no name:de → step 3 prefers int_name before name:en.
        let t = tags(&[
            ("name", "تونس"),
            ("int_name", "Tunis"),
            ("name:en", "Tunis EN"),
        ]);
        let sel = localized_name_with_secondary(&t, "de").unwrap();
        assert_eq!(sel.primary, "Tunis");
        assert_eq!(sel.secondary_local.as_deref(), Some("تونس"));
    }

    #[test]
    fn english_ui_prefers_name_en_over_int_name() {
        let t = tags(&[
            ("name", "تونس"),
            ("int_name", "Tunis"),
            ("name:en", "Tunis EN"),
        ]);
        assert_eq!(localized_name(&t, "en").as_deref(), Some("Tunis EN"));
    }

    #[test]
    fn latin_ui_uses_name_en_after_int_name() {
        let t = tags(&[("name", "القاهرة"), ("name:en", "Cairo")]);
        assert_eq!(localized_name(&t, "de").as_deref(), Some("Cairo"));
    }

    #[test]
    fn latin_ui_uses_name_fr_when_no_en_or_int_name() {
        let t = tags(&[
            ("name", "الطريق السريعة تونس - سوسة - صفاقس"),
            ("name:fr", "Autoroute Tunis - Sousse - Sfax"),
            ("name:ar", "الطريق السريعة تونس - سوسة - صفاقس"),
        ]);
        assert_eq!(
            localized_name(&t, "en").as_deref(),
            Some("Autoroute Tunis - Sousse - Sfax")
        );
        // Prefer fr over later Latin langs in the fixed list.
        let t2 = tags(&[
            ("name", "شارع الحبيب بورقيبة"),
            ("name:de", "Habib-Bourguiba-Straße"),
            ("name:fr", "Avenue Habib Bourguiba"),
        ]);
        assert_eq!(
            localized_name(&t2, "en").as_deref(),
            Some("Avenue Habib Bourguiba")
        );
    }

    #[test]
    fn arabic_ui_ignores_latin_name_xx_fallback() {
        let t = tags(&[
            ("name", "شارع الحبيب بورقيبة"),
            ("name:fr", "Avenue Habib Bourguiba"),
            ("name:en", "Habib Bourguiba Avenue"),
        ]);
        assert_eq!(
            localized_name(&t, "ar").as_deref(),
            Some("شارع الحبيب بورقيبة")
        );
    }

    #[test]
    fn arabic_ui_keeps_local_name() {
        let t = tags(&[
            ("name", "تونس"),
            ("name:en", "Tunis"),
            ("int_name", "Tunis"),
        ]);
        assert_eq!(localized_name(&t, "ar").as_deref(), Some("تونس"));
    }

    #[test]
    fn arabic_ui_prefers_name_ar() {
        let t = tags(&[("name", "Tunis"), ("name:ar", "تونس"), ("name:en", "Tunis")]);
        assert_eq!(localized_name(&t, "ar-TN").as_deref(), Some("تونس"));
    }

    #[test]
    fn falls_back_to_name() {
        let t = tags(&[("name", "Espa")]);
        assert_eq!(localized_name(&t, "de").as_deref(), Some("Espa"));
    }

    #[test]
    fn empty_tags_none() {
        let t = tags(&[]);
        assert!(localized_name(&t, "en").is_none());
    }

    #[test]
    fn latin_script_detection() {
        assert!(is_mostly_latin_script("Chaussée d'Antin"));
        assert!(is_mostly_latin_script("Mjøsvegen"));
        assert!(!is_mostly_latin_script("شارع الحبيب بورقيبة"));
        assert!(!is_mostly_latin_script("東京"));
    }
}
