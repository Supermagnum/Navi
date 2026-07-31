//! Read individual tiles from a local PMTiles archive (sync wrapper).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use pmtiles::{AsyncPmTilesReader, MmapBackend, TileCoord};
use tokio::runtime::Runtime;

fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("pmtiles-tile")
            .build()
            .expect("pmtiles tile runtime")
    })
}

type Reader = Arc<AsyncPmTilesReader<MmapBackend>>;

fn readers() -> &'static Mutex<HashMap<PathBuf, Reader>> {
    static READERS: OnceLock<Mutex<HashMap<PathBuf, Reader>>> = OnceLock::new();
    READERS.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn open_reader(path: &Path) -> anyhow::Result<Reader> {
    let backend = MmapBackend::try_from(path).await?;
    Ok(Arc::new(AsyncPmTilesReader::try_from_source(backend).await?))
}

/// Fetch one tile (decompressed) from a local PMTiles file.
///
/// Returns `Ok(None)` when the archive has no tile at that coordinate.
pub fn read_pmtiles_tile(path: &Path, z: u8, x: u32, y: u32) -> anyhow::Result<Option<Vec<u8>>> {
    let path_buf = path.to_path_buf();
    let coord = TileCoord::new(z, x, y).map_err(|e| anyhow::anyhow!("bad tile coord: {e}"))?;
    runtime().block_on(async {
        let reader = {
            let guard = readers()
                .lock()
                .map_err(|_| anyhow::anyhow!("pmtiles reader lock poisoned"))?;
            if let Some(existing) = guard.get(&path_buf) {
                existing.clone()
            } else {
                // Release lock before open? open is async; keep critical section short
                // by opening then inserting (double-checked under lock).
                drop(guard);
                let opened = open_reader(&path_buf).await?;
                let mut guard = readers()
                    .lock()
                    .map_err(|_| anyhow::anyhow!("pmtiles reader lock poisoned"))?;
                guard
                    .entry(path_buf.clone())
                    .or_insert_with(|| opened.clone())
                    .clone()
            }
        };
        Ok(reader
            .get_tile_decompressed(coord)
            .await?
            .map(|b| b.to_vec()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_mapterhorn_dem_tile_when_fixture_present() {
        let dem = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/integration-fixtures/europe_norway_ostlandet_dem.pmtiles");
        if !dem.is_file() {
            eprintln!("skip: missing {dem:?}");
            return;
        }
        // Gjendebu area z12.
        let bytes = read_pmtiles_tile(&dem, 12, 2143, 1154)
            .expect("read")
            .expect("tile present");
        assert!(bytes.len() > 1_000, "tile too small: {}", bytes.len());
        // WebP magic.
        assert_eq!(&bytes[0..4], b"RIFF");
    }
}
