//! Golden + negative tests for the NVSC/EL02 reader.
//!
//! Hedmark fixture is **vendored** from navi-server
//! `docs/examples/fixtures/Norway_Hedmark.navi-sidecar.v1.zst` into
//! `core/tests/fixtures/mwm/` so CI golden tests stay offline and bit-identical
//! to the server docs check (`4feb60…` / 267737 bytes).

#[cfg(test)]
mod golden {
    use crate::mwm::catalog::MwmFingerprint;
    use crate::mwm::checksum::sha256_hex_of_bytes;
    use crate::mwm::sidecar::{
        encode_test_sidecar_zstd, SidecarError, SidecarReader, WetlandRecord, CLASS_HARD,
        CLASS_SOFT,
    };

    const HEDMARK_ZST: &[u8] =
        include_bytes!("../../tests/fixtures/mwm/Norway_Hedmark.navi-sidecar.v1.zst");
    const HEDMARK_META: &str =
        include_str!("../../tests/fixtures/mwm/Norway_Hedmark.sidecar.meta.json");

    #[test]
    fn hedmark_fixture_checksum_and_inspect() {
        assert_eq!(HEDMARK_ZST.len(), 267_737);
        assert_eq!(
            sha256_hex_of_bytes(HEDMARK_ZST),
            "4feb60e8441eee228bb77d5d028e49c3a47e7cae5105e4d0a12e0485ee98363f"
        );
        let meta: serde_json::Value = serde_json::from_str(HEDMARK_META).unwrap();
        let r = SidecarReader::open_bytes(HEDMARK_ZST).unwrap();
        let h = r.header();
        assert_eq!(h.format_version, 1);
        assert_eq!(h.generator_tag, "comaps-release/2026.10.27");
        assert_eq!(h.generator_sha, "261105");
        assert_eq!(h.mwm_size, 102_194_292);
        assert_eq!(
            h.mwm_sha256_hex(),
            "2852a363342183bf4df6b1b78f9790685dd7e6a1bc34eaefbf177fc6432799af"
        );
        let counts = r.inspect_counts().unwrap();
        let insp = &meta["inspect"];
        assert_eq!(
            counts.wetland_records,
            insp["wetland_records"].as_u64().unwrap() as usize
        );
        assert_eq!(
            counts.soft_records,
            insp["soft_records"].as_u64().unwrap() as usize
        );
        assert_eq!(
            counts.hard_records,
            insp["hard_records"].as_u64().unwrap() as usize
        );
        assert_eq!(
            counts.boardwalk_records,
            insp["boardwalk_records"].as_u64().unwrap() as usize
        );
        assert_eq!(
            counts.elevation_features,
            insp["elevation_features"].as_u64().unwrap() as usize
        );
        assert_eq!(
            counts.elevation_points,
            insp["elevation_points"].as_u64().unwrap() as usize
        );
        assert_eq!(
            counts.sections,
            vec![
                "wetland_edges",
                "boardwalk_flags",
                "elevation_overrides",
                "attribution"
            ]
        );
        // Sample wetland records from docs.
        assert_eq!(r.wetland_class(495, 45, 0).unwrap(), Some(CLASS_SOFT));
        assert_eq!(r.wetland_class(495, 45, 1).unwrap(), Some(CLASS_SOFT));
        assert_eq!(r.wetland_class(495, 46, 0).unwrap(), Some(CLASS_SOFT));
        let recs = r.wetland_records().unwrap();
        assert_eq!(
            &recs[0],
            &WetlandRecord {
                feature_id: 495,
                segment_index: 45,
                direction: 0,
                class: CLASS_SOFT,
            }
        );
        assert!(recs.iter().any(|r| r.class == CLASS_HARD));
        // Attribution parses as JSON.
        let attr = r.attribution_value().unwrap();
        assert!(attr.is_object());
        r.require_fingerprint(&MwmFingerprint {
            size: 102_194_292,
            sha256: h.mwm_sha256_hex(),
        })
        .unwrap();
    }

    #[test]
    fn bad_section_checksum_rejected() {
        let zst = encode_test_sidecar_zstd(
            "t",
            "s",
            [2; 32],
            10,
            &[WetlandRecord {
                feature_id: 1,
                segment_index: 0,
                direction: 0,
                class: CLASS_SOFT,
            }],
            &[],
            &[],
            "{}",
        )
        .unwrap();
        let mut logical = zstd::stream::decode_all(zst.as_slice()).unwrap();
        // Directory: after wetland name "wetland_edges" (u16-len + bytes) come
        // uncompressed u64, compressed u64, then sha256. Flip one sha byte.
        let name = b"wetland_edges";
        let mut needle = Vec::new();
        needle.extend_from_slice(&(name.len() as u16).to_le_bytes());
        needle.extend_from_slice(name);
        let pos = logical
            .windows(needle.len())
            .position(|w| w == needle.as_slice())
            .expect("wetland dir entry");
        let sha_off = pos + needle.len() + 8 + 8;
        logical[sha_off] ^= 0xff;
        let bad = zstd::stream::encode_all(logical.as_slice(), 3).unwrap();
        let r = SidecarReader::open_bytes(&bad).unwrap();
        let err = r.wetland_records().unwrap_err();
        assert!(
            matches!(err, SidecarError::SectionChecksumMismatch(_)),
            "{err:?}"
        );
    }

    #[test]
    fn missing_required_section_rejected() {
        // Attribution-only logical blob without wetland directory entry.
        let mut logical = Vec::new();
        logical.extend_from_slice(b"NVSC");
        logical.extend_from_slice(&1u32.to_le_bytes());
        let tag = b"t";
        logical.extend_from_slice(&(tag.len() as u16).to_le_bytes());
        logical.extend_from_slice(tag);
        let sha = b"s";
        logical.extend_from_slice(&(sha.len() as u16).to_le_bytes());
        logical.extend_from_slice(sha);
        logical.extend_from_slice(&[0u8; 32]);
        logical.extend_from_slice(&1u64.to_le_bytes());
        logical.extend_from_slice(&0u32.to_le_bytes()); // no sections
        let outer = zstd::stream::encode_all(logical.as_slice(), 3).unwrap();
        assert!(matches!(
            SidecarReader::open_bytes(&outer),
            Err(SidecarError::MissingRequiredSection(_))
        ));
    }

    #[test]
    fn unsupported_format_version_rejected() {
        let mut logical = zstd::stream::decode_all(
            encode_test_sidecar_zstd("t", "s", [0; 32], 1, &[], &[], &[], "{}")
                .unwrap()
                .as_slice(),
        )
        .unwrap();
        logical[4..8].copy_from_slice(&99u32.to_le_bytes());
        let outer = zstd::stream::encode_all(logical.as_slice(), 3).unwrap();
        assert!(matches!(
            SidecarReader::open_bytes(&outer),
            Err(SidecarError::UnsupportedVersion(99))
        ));
    }
}
