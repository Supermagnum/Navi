# Note for navi-server `docs/client-fetch-mwm.md` (apply upstream)

Text-only proposal for the client storage / World overlay rule. Do not merge
this file into navi-server from the Navi client PR — paste into
`docs/client-fetch-mwm.md` on Supermagnum/navi-server when convenient.

---

## World / WorldCoasts storage (client)

`World.mwm` and `WorldCoasts.mwm` are CoMaps non-routable overlays
(`no_sidecar: true`). Clients must:

1. **Download them with every release** (same `version` / checksum path as
   country leaves; no Navi sidecar).
2. **Store them at the MWM storage root**, not under
   `releases/{version}/leaves/`. Stock CoMaps `GetAdditionalWorldScope` is
   resource-only (`r`). Navi quilt patch
   `0011-navi-world-writable-scope.patch` widens it to **`wr`** so root
   `World.mwm` / `WorldCoasts.mwm` are found for Framework/search without
   treating them as versioned country leaves under `261105/` (those lack
   cross-mwm sections and break Joints). Separately,
   `0012-navi-routes-builder-skip-world.patch` skips registering World /
   WorldCoasts in headless `RoutesBuilder` so mixed World+country graphs do
   not yield `No route` / `build_s=0` on Car long-trip.
3. **Record ownership** in a sibling pointer (e.g. `world-release.json` with
   `{"version":"261105"}`) that must match `active.json` before routing.
4. **Switch atomically** with the leaf set: verify new World bytes in staging,
   promote root World files + `world-release.json` + `active.json` only after
   staging is complete. On interrupted download or failed activate, keep the
   previous active release and the previous root World files intact.
5. **Refuse to route** when `world-release` ≠ `active` or when root World files
   are missing.

Suggested layout fragment:

```text
{data_dir}/mwm/
  active.json
  world-release.json    # must match active.version
  World.mwm
  WorldCoasts.mwm
  releases/{version}/leaves/{Country}.mwm   # no World* here after activate
```
