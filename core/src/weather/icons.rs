//! Fill-style Meteocons path resolution (v1: static fill only).

use std::path::{Path, PathBuf};

/// Runtime style used for HUD icons in this release.
pub const WEATHER_ICON_STYLE_FILL: &str = "fill";

/// Default style for the weather plugin until other styles are wired.
pub const DEFAULT_WEATHER_ICON_STYLE: &str = WEATHER_ICON_STYLE_FILL;

/// Preferred fallback when a slug is missing or unmapped.
pub const FALLBACK_WEATHER_ICON_SLUG: &str = "not-available";

/// Secondary fallback if `not-available` is absent from the tree.
const UNKNOWN_FALLBACK_SLUG: &str = "unknown";

/// Relative icon path under a weather icons root (`…/icons` or assets `icons/weather`).
///
/// Single source of truth for the on-disk layout so styles can be swapped later
/// without hunting string literals.
pub fn weather_icon_relative_path(slug: &str) -> String {
    weather_icon_relative_path_for_style(DEFAULT_WEATHER_ICON_STYLE, slug)
}

fn weather_icon_relative_path_for_style(style: &str, slug: &str) -> String {
    let safe = sanitize_slug(slug);
    format!("{style}/{safe}.svg")
}

fn sanitize_slug(slug: &str) -> &str {
    if slug.is_empty()
        || slug.contains("..")
        || slug.contains('/')
        || slug.contains('\\')
        || !slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        FALLBACK_WEATHER_ICON_SLUG
    } else {
        slug
    }
}

/// Resolve an absolute SVG path under `icons_root`, falling back to
/// `not-available` then `unknown`.
pub fn resolve_weather_icon_path(icons_root: &Path, slug: &str) -> PathBuf {
    let primary = icons_root.join(weather_icon_relative_path(slug));
    if primary.is_file() {
        return primary;
    }
    let na = icons_root.join(weather_icon_relative_path(FALLBACK_WEATHER_ICON_SLUG));
    if na.is_file() {
        return na;
    }
    icons_root.join(weather_icon_relative_path(UNKNOWN_FALLBACK_SLUG))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fill_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../plugins/weather/icons")
    }

    #[test]
    fn relative_path_uses_fill_style_once() {
        assert_eq!(
            weather_icon_relative_path("clear-day"),
            "fill/clear-day.svg"
        );
        assert_eq!(DEFAULT_WEATHER_ICON_STYLE, WEATHER_ICON_STYLE_FILL);
    }

    #[test]
    fn resolves_clear_day_and_fallback() {
        let root = fill_root();
        if !root.join("fill").is_dir() {
            return;
        }
        let p = resolve_weather_icon_path(&root, "clear-day");
        assert!(p.ends_with("fill/clear-day.svg"));
        assert!(p.is_file());
        let missing = resolve_weather_icon_path(&root, "this-slug-does-not-exist-xyz");
        assert!(
            missing.ends_with("fill/not-available.svg") || missing.ends_with("fill/unknown.svg")
        );
        assert!(missing.is_file());
    }

    #[test]
    fn rejects_path_traversal_slug() {
        assert_eq!(
            weather_icon_relative_path("../etc/passwd"),
            "fill/not-available.svg"
        );
    }
}
