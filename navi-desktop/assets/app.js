/* Navi Linux desktop map shell — MapLibre GL JS + local /api */

(function () {
  const protocol = new pmtiles.Protocol();
  maplibregl.addProtocol("pmtiles", protocol.tile);

  let map = null;
  let posMarker = null;
  let routeSourceReady = false;
  let lastStyleUrl = null;

  function parseLatLon(text) {
    const parts = text.split(/[, ]+/).map((s) => s.trim()).filter(Boolean);
    if (parts.length < 2) throw new Error("need lat,lon");
    return { lat: Number(parts[0]), lon: Number(parts[1]) };
  }

  function polylineToCoords(poly) {
    // "lon,lat;lon,lat;..."
    return poly
      .split(";")
      .map((p) => p.trim())
      .filter(Boolean)
      .map((p) => {
        const [lon, lat] = p.split(",").map(Number);
        return [lon, lat];
      });
  }

  async function ensureMap(styleUrl, center) {
    if (map && lastStyleUrl === styleUrl) return;
    lastStyleUrl = styleUrl;
    if (map) {
      map.remove();
      map = null;
      posMarker = null;
      routeSourceReady = false;
    }
    map = new maplibregl.Map({
      container: "map",
      style: styleUrl,
      center: center || [10.7, 60.8],
      zoom: 7,
    });
    map.addControl(new maplibregl.NavigationControl(), "top-right");
    await map.once("load");
    map.addSource("route", {
      type: "geojson",
      data: { type: "FeatureCollection", features: [] },
    });
    map.addLayer({
      id: "route-line",
      type: "line",
      source: "route",
      paint: {
        "line-color": "#1a73e8",
        "line-width": 5,
        "line-opacity": 0.9,
      },
    });
    map.addSource("pois", {
      type: "geojson",
      data: { type: "FeatureCollection", features: [] },
    });
    map.addLayer({
      id: "poi-circles",
      type: "circle",
      source: "pois",
      paint: {
        "circle-radius": 7,
        "circle-color": "#e67e22",
        "circle-stroke-width": 2,
        "circle-stroke-color": "#fff",
      },
    });
    routeSourceReady = true;
  }

  function setRoute(polyline, breakPoisJson) {
    if (!map || !routeSourceReady) return;
    const coords = polylineToCoords(polyline);
    map.getSource("route").setData({
      type: "Feature",
      geometry: { type: "LineString", coordinates: coords },
    });
    let pois = [];
    try {
      pois = JSON.parse(breakPoisJson || "[]");
    } catch (_) {}
    map.getSource("pois").setData({
      type: "FeatureCollection",
      features: pois.map((p) => ({
        type: "Feature",
        properties: { name: p.name || "", icon: p.icon || "" },
        geometry: { type: "Point", coordinates: [p.lon, p.lat] },
      })),
    });
    if (coords.length > 1) {
      const bounds = coords.reduce(
        (b, c) => b.extend(c),
        new maplibregl.LngLatBounds(coords[0], coords[0])
      );
      map.fitBounds(bounds, { padding: 48, maxZoom: 12 });
    }
  }

  function setPosition(lat, lon) {
    if (!map) return;
    if (!posMarker) {
      const el = document.createElement("div");
      el.className = "pos-dot";
      el.style.width = "14px";
      el.style.height = "14px";
      el.style.borderRadius = "50%";
      el.style.background = "#2ecc71";
      el.style.border = "2px solid #fff";
      el.style.boxShadow = "0 0 4px rgba(0,0,0,0.4)";
      posMarker = new maplibregl.Marker({ element: el }).setLngLat([lon, lat]).addTo(map);
    } else {
      posMarker.setLngLat([lon, lat]);
    }
  }

  function updateHud(hud) {
    document.getElementById("hudTurn").textContent = hud.next_turn || "—";
    document.getElementById("hudDist").textContent = hud.distance_to_turn || "—";
    document.getElementById("hudBreak").textContent = hud.distance_to_break || "—";
    document.getElementById("hudEco").textContent = hud.eco_active ? "on" : "off";
    document.getElementById("hudEta").textContent =
      hud.eta_minutes != null
        ? `${Math.round(hud.eta_minutes)} min · ${Number(hud.distance_km || 0).toFixed(1)} km`
        : "—";
    const icon = document.getElementById("hudIcon");
    if (hud.next_turn_icon) {
      icon.src = `/api/icon/${encodeURIComponent(hud.next_turn_icon)}`;
      icon.style.display = "block";
    }
  }

  async function pollStatus() {
    const res = await fetch("/api/status");
    const st = await res.json();
    const center = st.position
      ? [st.position.lon, st.position.lat]
      : [10.7, 60.8];
    await ensureMap(st.basemap.style_url, center);
    document.getElementById("basemapNote").textContent =
      `${st.basemap.kind}: ${st.basemap.note}`;
    if (st.position) setPosition(st.position.lat, st.position.lon);
    updateHud(st.hud || {});
    if (st.route && st.route.route_polyline) {
      setRoute(st.route.route_polyline, st.route.break_pois_json);
    }
  }

  document.getElementById("plan").onclick = async () => {
    const log = document.getElementById("planLog");
    log.textContent = "Planning…";
    try {
      const s = parseLatLon(document.getElementById("start").value);
      const e = parseLatLon(document.getElementById("end").value);
      const body = {
        start_lat: s.lat,
        start_lon: s.lon,
        end_lat: e.lat,
        end_lon: e.lon,
        use_eco: document.getElementById("eco").checked,
        profile: document.getElementById("profile").value,
      };
      const res = await fetch("/api/plan", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
      const data = await res.json();
      if (!data.ok) {
        log.textContent = data.error || "plan failed";
        return;
      }
      log.textContent =
        `OK ${data.distance_km.toFixed(1)} km · ETA ${Math.round(data.eta_minutes)} min\n` +
        (data.report || "").slice(0, 1200);
      setRoute(data.route_polyline, data.break_pois_json);
    } catch (err) {
      log.textContent = String(err);
    }
  };

  let searchTimer = null;
  document.getElementById("search").addEventListener("input", (ev) => {
    clearTimeout(searchTimer);
    const q = ev.target.value.trim();
    searchTimer = setTimeout(async () => {
      const ul = document.getElementById("hits");
      ul.innerHTML = "";
      if (q.length < 2) return;
      const res = await fetch(`/api/search?q=${encodeURIComponent(q)}`);
      const data = await res.json();
      (data.hits || []).forEach((h) => {
        const li = document.createElement("li");
        const parts = [h.name, h.sub_area, h.municipality].filter((p) => p && String(p).trim());
        const seen = [];
        parts.forEach((p) => {
          const t = String(p).trim();
          if (!seen.some((s) => s.toLowerCase() === t.toLowerCase())) seen.push(t);
        });
        li.textContent = `${seen.join(", ")} (${h.kind})`;
        li.onclick = () => {
          document.getElementById("end").value = `${h.lat},${h.lon}`;
        };
        ul.appendChild(li);
      });
    }, 250);
  });

  pollStatus().catch(console.error);
  setInterval(() => pollStatus().catch(console.error), 1000);
})();
