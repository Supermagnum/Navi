fn main() -> anyhow::Result<()> {
    use driver_break_core::routing::indexed::{ensure_tile_index, tile_index_path};
    use std::collections::HashSet;
    use std::env;
    use std::path::PathBuf;
    use std::time::Instant;
    let path = PathBuf::from(env::args().nth(1).expect("tile path"));
    let border = HashSet::new();
    // force rebuild
    let _ = std::fs::remove_file(tile_index_path(&path));
    let t0 = Instant::now();
    let (ms, bytes) = ensure_tile_index(&path, &border)?;
    println!(
        "PASS path={} reported_ms={ms} wall_ms={} out_bytes={bytes}",
        path.display(),
        t0.elapsed().as_millis()
    );
    Ok(())
}
