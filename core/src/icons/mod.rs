//! Semantic icon key resolution over the bundled Navit-derived SVG set.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use resvg::tiny_skia;
use resvg::usvg;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconTheme {
    Day,
    Night,
}

/// Resolve a semantic key to an on-disk SVG (or SVGZ) path.
///
/// Order: `override_dir` -> bundled `core/src/icons` -> placeholder `unknown.svg`.
pub fn resolve_icon(
    key: &str,
    theme: IconTheme,
    override_dir: Option<&Path>,
    bundled_dir: &Path,
) -> PathBuf {
    let candidates = candidate_filenames(key, theme);
    if let Some(dir) = override_dir {
        for name in &candidates {
            let p = dir.join(name);
            if p.exists() {
                return p;
            }
        }
    }
    for name in &candidates {
        let p = bundled_dir.join(name);
        if p.exists() {
            return p;
        }
    }
    bundled_dir.join("unknown.svg")
}

/// Load SVG bytes from path; if extension is `.svgz`, gunzip first.
pub fn load_svg_bytes(path: &Path) -> Result<Vec<u8>> {
    let raw = fs::read(path).with_context(|| format!("read icon file {}", path.display()))?;
    if is_svgz(path) {
        let mut decoder = GzDecoder::new(raw.as_slice());
        let mut svg = Vec::new();
        decoder
            .read_to_end(&mut svg)
            .context("gunzip .svgz icon data")?;
        Ok(svg)
    } else {
        Ok(raw)
    }
}

/// Rasterize SVG/SVGZ file to RGBA8 pixels, row-major, length = w*h*4.
///
/// Pixel data is premultiplied RGBA as produced by `resvg`/`tiny-skia`.
pub fn rasterize_file(path: &Path, width: u32, height: u32) -> Result<Vec<u8>> {
    let svg_bytes = load_svg_bytes(path)?;
    let resources_dir = path.parent();
    Ok(render_to_pixmap(&svg_bytes, resources_dir, width, height)?
        .data()
        .to_vec())
}

/// Resolve semantic key then rasterize. `bundled_dir` is usually
/// `CARGO_MANIFEST_DIR/src/icons` or a runtime asset path.
pub fn rasterize_key(
    key: &str,
    theme: IconTheme,
    width: u32,
    height: u32,
    override_dir: Option<&Path>,
    bundled_dir: &Path,
) -> Result<Vec<u8>> {
    let path = resolve_icon(key, theme, override_dir, bundled_dir);
    rasterize_file(&path, width, height)
}

/// Same as [`rasterize_key`] but returns PNG bytes.
pub fn rasterize_key_png(
    key: &str,
    theme: IconTheme,
    width: u32,
    height: u32,
    override_dir: Option<&Path>,
    bundled_dir: &Path,
) -> Result<Vec<u8>> {
    let path = resolve_icon(key, theme, override_dir, bundled_dir);
    rasterize_file_png(&path, width, height)
}

fn rasterize_file_png(path: &Path, width: u32, height: u32) -> Result<Vec<u8>> {
    let svg_bytes = load_svg_bytes(path)?;
    let resources_dir = path.parent();
    let pixmap = render_to_pixmap(&svg_bytes, resources_dir, width, height)?;
    pixmap.encode_png().context("encode rasterized icon as PNG")
}

fn render_to_pixmap(
    svg_bytes: &[u8],
    resources_dir: Option<&Path>,
    width: u32,
    height: u32,
) -> Result<tiny_skia::Pixmap> {
    if width == 0 || height == 0 {
        anyhow::bail!("raster size must be non-zero");
    }

    let mut opt = usvg::Options::default();
    if let Some(dir) = resources_dir {
        opt.resources_dir = Some(dir.to_path_buf());
    }

    let tree = usvg::Tree::from_data(svg_bytes, &opt).context("parse SVG")?;
    let mut pixmap = tiny_skia::Pixmap::new(width, height).context("allocate pixmap")?;
    pixmap.fill(tiny_skia::Color::TRANSPARENT);

    let size = tree.size();
    let sx = width as f32 / size.width();
    let sy = height as f32 / size.height();
    let transform = tiny_skia::Transform::from_scale(sx, sy);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    Ok(pixmap)
}

fn is_svgz(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("svgz"))
}

fn candidate_filenames(key: &str, theme: IconTheme) -> Vec<String> {
    let suffix = match theme {
        IconTheme::Day => "_bk",
        IconTheme::Night => "_wh",
    };
    let mut out = Vec::new();
    // Themed variants first for nav_/status_ keys.
    if key.starts_with("nav_") || key.starts_with("status_") {
        out.push(format!("{key}{suffix}.svg"));
    }
    out.push(format!("{key}.svg"));
    out.push(format!("{key}.svgz"));
    // Common semantic aliases -> Navit filenames.
    match key {
        "water" | "drinking_water" => out.push("drinking_water.svg".into()),
        "eco" | "eco-mode" => out.push("leaf.svg".into()),
        "cabin" | "shelter" => out.push("shelter.svg".into()),
        "fuel" => out.push("fuel.svg".into()),
        "toilets" | "restroom" => out.push("toilets.svg".into()),
        "leisure-fishing" | "fishing" | "fish" => out.push("fish.svg".into()),
        // Self-authored speed camera mark (docs/icons.md); OSM-tag-key filename.
        "speed_camera" | "speed-camera" | "enforcement_maxspeed" | "highway-speed_camera" => {
            out.push("speed_camera.svg".into())
        }
        _ => {}
    }
    if let Some(rest) = key.strip_prefix("country_") {
        out.push(format!("country_{rest}.svgz"));
        out.push(format!("country_{}.svgz", rest.to_uppercase()));
        out.push(format!("country_{}.svgz", rest.to_lowercase()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    fn bundled_icons_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/icons")
    }

    fn assert_nontrivial_rgba(rgba: &[u8], width: u32, height: u32) {
        assert_eq!(rgba.len(), (width * height * 4) as usize);
        assert!(
            !rgba.iter().all(|&b| b == 0),
            "expected at least one non-zero byte"
        );
        let alpha_sum: u64 = rgba.chunks_exact(4).map(|px| px[3] as u64).sum();
        assert!(alpha_sum > 0, "expected non-trivial alpha channel");
        let alpha_pixels = rgba.chunks_exact(4).filter(|px| px[3] > 0).count();
        assert!(alpha_pixels > 0, "expected pixels with non-zero alpha");
    }

    #[test]
    fn resolves_leaf_for_eco() {
        let bundled = bundled_icons_dir();
        let p = resolve_icon("eco-mode", IconTheme::Day, None, &bundled);
        assert!(p.ends_with("leaf.svg") || p.ends_with("unknown.svg"));
    }

    #[test]
    fn rasterizes_leaf_svg() {
        let bundled = bundled_icons_dir();
        let path = bundled.join("leaf.svg");
        let width = 48;
        let height = 48;
        let rgba = rasterize_file(&path, width, height).expect("leaf.svg rasterize");
        assert_nontrivial_rgba(&rgba, width, height);
    }

    #[test]
    fn resolves_and_rasterizes_speed_camera() {
        let bundled = bundled_icons_dir();
        let p = resolve_icon("speed_camera", IconTheme::Day, None, &bundled);
        assert!(
            p.ends_with("speed_camera.svg"),
            "must not fall back to unknown: {}",
            p.display()
        );
        let rgba = rasterize_key(
            "enforcement_maxspeed",
            IconTheme::Day,
            64,
            64,
            None,
            &bundled,
        )
        .expect("speed_camera rasterize");
        assert_nontrivial_rgba(&rgba, 64, 64);
    }

    #[test]
    fn rasterizes_nav_icon_by_key() {
        let bundled = bundled_icons_dir();
        let width = 64;
        let height = 64;
        let rgba = rasterize_key(
            "nav_straight",
            IconTheme::Day,
            width,
            height,
            None,
            &bundled,
        )
        .expect("nav_straight rasterize");
        assert_nontrivial_rgba(&rgba, width, height);
    }

    #[test]
    fn rasterizes_poi_fuel_by_key() {
        let bundled = bundled_icons_dir();
        let width = 32;
        let height = 32;
        let rgba = rasterize_key("fuel", IconTheme::Day, width, height, None, &bundled)
            .expect("fuel rasterize");
        assert_nontrivial_rgba(&rgba, width, height);
    }

    #[test]
    fn rasterizes_gzipped_svgz_country_flag() {
        let dir = tempfile::tempdir().expect("tempdir");
        let svgz_path = dir.path().join("country_no.svgz");
        let svg = r###"<svg xmlns="http://www.w3.org/2000/svg" width="22" height="16"><rect width="22" height="16" fill="#BA0C2F"/><rect x="6" width="4" height="16" fill="#FFFFFF"/><rect y="6" width="22" height="4" fill="#FFFFFF"/><rect x="7" width="2" height="16" fill="#00205B"/><rect y="7" width="22" height="2" fill="#00205B"/></svg>"###;
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(svg.as_bytes()).expect("gzip svg");
        fs::write(&svgz_path, encoder.finish().expect("finish gzip")).expect("write svgz");

        let loaded = load_svg_bytes(&svgz_path).expect("load svgz");
        assert!(loaded.starts_with(b"<svg"));

        let width = 44;
        let height = 32;
        let rgba = rasterize_file(&svgz_path, width, height).expect("svgz rasterize");
        assert_nontrivial_rgba(&rgba, width, height);

        let png = rasterize_file_png(&svgz_path, width, height).expect("svgz png");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    }
}
