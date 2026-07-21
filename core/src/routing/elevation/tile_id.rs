use std::fmt;

/// 1-degree HGT tile identifier (e.g. N61E009).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HgtTileId {
    pub lat_floor: i32,
    pub lon_floor: i32,
}

impl HgtTileId {
    pub fn from_lat_lon(lat: f64, lon: f64) -> Self {
        Self {
            lat_floor: lat.floor() as i32,
            lon_floor: lon.floor() as i32,
        }
    }

    pub fn stem(&self) -> String {
        let ns = if self.lat_floor >= 0 { 'N' } else { 'S' };
        let ew = if self.lon_floor >= 0 { 'E' } else { 'W' };
        format!(
            "{ns}{:02}{ew}{:03}",
            self.lat_floor.abs(),
            self.lon_floor.abs()
        )
    }
}

impl fmt::Display for HgtTileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.stem())
    }
}

/// Expand a bounding box [min_lat, min_lon, max_lat, max_lon] to tile ids.
pub fn bbox_to_tiles(bbox: [f64; 4]) -> Vec<HgtTileId> {
    let min_lat = bbox[0].floor() as i32;
    let min_lon = bbox[1].floor() as i32;
    let max_lat = bbox[2].floor() as i32;
    let max_lon = bbox[3].floor() as i32;
    let mut tiles = Vec::new();
    for lat in min_lat..=max_lat {
        for lon in min_lon..=max_lon {
            tiles.push(HgtTileId {
                lat_floor: lat,
                lon_floor: lon,
            });
        }
    }
    tiles.sort_unstable();
    tiles.dedup();
    tiles
}

impl HgtTileId {
    pub fn parse_stem(stem: &str) -> Option<Self> {
        if stem.len() != 7 {
            return None;
        }
        let bytes = stem.as_bytes();
        let lat_sign = match bytes[0] {
            b'N' => 1,
            b'S' => -1,
            _ => return None,
        };
        let lat: i32 = std::str::from_utf8(&bytes[1..3])
            .ok()?
            .parse::<i32>()
            .ok()?
            * lat_sign;
        let lon_sign = match bytes[3] {
            b'E' => 1,
            b'W' => -1,
            _ => return None,
        };
        let lon: i32 = std::str::from_utf8(&bytes[4..7])
            .ok()?
            .parse::<i32>()
            .ok()?
            * lon_sign;
        Some(Self {
            lat_floor: lat,
            lon_floor: lon,
        })
    }
}

impl std::str::FromStr for HgtTileId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_stem(s).ok_or_else(|| anyhow::anyhow!("invalid tile stem {s}"))
    }
}

/// Known country bounding boxes for entire-country elevation jobs.
pub fn country_bbox(country_code: &str) -> Option<[f64; 4]> {
    crate::routing::elevation::country::lookup(country_code)
}
