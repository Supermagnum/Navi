//! The guest decides from parsed records. It does not open files or the network.

#[test]
fn guest_sources_have_no_paths_or_http_client() {
    let files = ["../src/lib.rs", "../src/follow.rs", "../src/select.rs"];
    for rel in files {
        let src = match rel {
            "../src/lib.rs" => include_str!("../src/lib.rs"),
            "../src/follow.rs" => include_str!("../src/follow.rs"),
            "../src/select.rs" => include_str!("../src/select.rs"),
            _ => unreachable!(),
        };
        assert!(!src.contains("std::fs"), "{rel}");
        assert!(!src.contains("std::net"), "{rel}");
        assert!(!src.contains("reqwest"), "{rel}");
        assert!(!src.contains("files/repeaters"), "{rel}");
        assert!(!src.contains("repeaterbook.com"), "{rel}");
    }
}
