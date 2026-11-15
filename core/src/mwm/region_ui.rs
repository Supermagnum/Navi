//! Region UI data from `country_groups` + `display_name_overrides`.

use super::catalog::{CountryGroup, DisplayNameOverrides, MwmCatalog};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionUiGroup {
    pub id: String,
    pub display_name: String,
    pub parent_groups: Vec<String>,
    pub leaves: Vec<RegionUiLeaf>,
    pub subgroups: Vec<RegionUiGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionUiLeaf {
    pub id: String,
    pub display_name: String,
    pub comaps_display_name: Option<String>,
    pub display_name_overridden: bool,
    pub mwm_bytes: Option<u64>,
    pub sidecar_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    /// True for World / WorldCoasts (and any catalog `no_sidecar` leaf).
    pub no_sidecar: bool,
}

fn override_name(overrides: &DisplayNameOverrides, leaf_id: &str) -> Option<String> {
    overrides
        .leaves
        .get(leaf_id)
        .map(|o| o.display_name.clone())
}

fn map_leaf(
    leaf_id: &str,
    display_name: Option<&str>,
    comaps_display_name: Option<&str>,
    already_overridden: bool,
    mwm_bytes: Option<u64>,
    sidecar_bytes: Option<u64>,
    total_bytes: Option<u64>,
    overrides: &DisplayNameOverrides,
    non_routable: &[String],
) -> RegionUiLeaf {
    let from_override = override_name(overrides, leaf_id);
    let overridden = already_overridden || from_override.is_some();
    let display = from_override
        .or_else(|| display_name.map(|s| s.to_string()))
        .unwrap_or_else(|| leaf_id.to_string());
    RegionUiLeaf {
        id: leaf_id.to_string(),
        display_name: display,
        comaps_display_name: comaps_display_name.map(|s| s.to_string()),
        display_name_overridden: overridden,
        mwm_bytes,
        sidecar_bytes,
        total_bytes,
        no_sidecar: non_routable.iter().any(|n| n == leaf_id),
    }
}

fn map_group(g: &CountryGroup, catalog: &MwmCatalog) -> RegionUiGroup {
    let leaves = g
        .leaves
        .iter()
        .map(|l| {
            let no_sidecar_leaf = catalog.leaf(&l.id).map(|c| c.no_sidecar).unwrap_or(false);
            let mut ui = map_leaf(
                &l.id,
                l.display_name.as_deref(),
                l.comaps_display_name.as_deref(),
                l.display_name_overridden.unwrap_or(false),
                l.mwm_bytes.or(l.comaps_bytes),
                l.sidecar_bytes,
                l.total_bytes,
                &catalog.display_name_overrides,
                &catalog.non_routable_leaves,
            );
            if no_sidecar_leaf {
                ui.no_sidecar = true;
            }
            ui
        })
        .collect();
    RegionUiGroup {
        id: g.id.clone(),
        display_name: g.display_name.clone().unwrap_or_else(|| g.id.clone()),
        parent_groups: g.parent_groups.clone(),
        leaves,
        subgroups: g
            .subgroups
            .iter()
            .map(|sg| map_group(sg, catalog))
            .collect(),
    }
}

/// Build the region-download UI tree from the catalog.
pub fn region_ui_from_catalog(catalog: &MwmCatalog) -> Vec<RegionUiGroup> {
    catalog
        .country_groups
        .iter()
        .map(|g| map_group(g, catalog))
        .collect()
}

/// Flatten unique leaf ids from country groups (order: DFS).
pub fn flatten_country_group_leaves(groups: &[RegionUiGroup]) -> Vec<RegionUiLeaf> {
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    fn walk(
        groups: &[RegionUiGroup],
        out: &mut Vec<RegionUiLeaf>,
        seen: &mut std::collections::BTreeSet<String>,
    ) {
        for g in groups {
            for leaf in &g.leaves {
                if seen.insert(leaf.id.clone()) {
                    out.push(leaf.clone());
                }
            }
            walk(&g.subgroups, out, seen);
        }
    }
    walk(groups, &mut out, &mut seen);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mwm::catalog::parse_catalog_json;

    #[test]
    fn applies_norway_overrides_from_example() {
        let c = parse_catalog_json(include_str!(
            "../../tests/fixtures/mwm/catalog-v2.example.json"
        ))
        .unwrap();
        let ui = region_ui_from_catalog(&c);
        assert_eq!(ui.len(), 1);
        assert_eq!(ui[0].id, "Norway");
        let hedmark = ui[0]
            .leaves
            .iter()
            .find(|l| l.id == "Norway_Hedmark")
            .unwrap();
        assert_eq!(hedmark.display_name, "Innlandet (Hedmark)");
        assert!(hedmark.display_name_overridden);
        assert!(!hedmark.no_sidecar);
        let flat = flatten_country_group_leaves(&ui);
        assert_eq!(flat.len(), 2);
    }
}
