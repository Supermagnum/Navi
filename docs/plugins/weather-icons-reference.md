# Meteocons weather icon reference (fill style)

**Path:** `docs/plugins/weather-icons-reference.md`
**Icons shown:** `plugins/weather/icons/fill/*.svg` (519 files)
**Source manifest:** `plugins/weather/icons/manifest.json`

Human-readable purpose for each Meteocons slug vendored for the Navi
weather plugin. The same slugs exist in `flat/`, `line/`, and
`monochrome/` with identical meaning — only the visual style differs.
Animated counterparts (where present) use the same slug under
`plugins/weather/animated-icons/fill/`.

Plugin spec: [`weather-plugin.md`](weather-plugin.md). Regenerate this
file after manifest updates:

```bash
python3 scripts/generate-weather-icons-reference.py
```

---

## How to read a slug

Most condition icons combine layers in the filename:

```text
{sky-cover}[-day|-night][-{precipitation-or-haze}]
thunderstorms-{sky-cover}[-day|-night][-{precipitation}]
extreme-thunderstorms-{sky-cover}[-day|-night][-{precipitation}]
```

| Token | Meaning |
|---|---|
| `mostly-clear` | Large clear area, some cloud |
| `partly-cloudy` | Sun/moon and cloud share the sky |
| `overcast` | Full cloud cover |
| `extreme` | Very heavy precipitation or storm rate |
| `thunderstorms` | Lightning present |
| `extreme-thunderstorms` | Severe thunderstorm |
| `-day` / `-night` | Sun or moon variant for map/HUD theme |
| `-drizzle` | Light drizzle |
| `-rain` | Rain |
| `-sleet` | Rain and snow mixed |
| `-snow` | Snow |
| `-hail` | Hail |
| `-fog` | Fog |
| `-haze` | Haze / reduced visibility |
| `-smoke` | Smoke or poor air quality |

Example: `partly-cloudy-day-rain` = partly cloudy sky during the day with rain.

---

## Standard

22 icons. File path: `plugins/weather/icons/fill/{slug}.svg`.

| Slug | Purpose |
|---|---|
| `clear-day` | Clear sky during the day |
| `clear-night` | Clear sky at night |
| `cloudy` | Overcast or fully cloudy |
| `drizzle` | Drizzle |
| `hail` | Hail |
| `mist` | Mist |
| `rain` | Rain |
| `sleet` | Sleet |
| `smoke` | Smoke or haze from fires |
| `snow` | Snow |
| `cloud-down` | Cloud base lowering / thickening cloud |
| `cloud-up` | Cloud base rising / thinning cloud |
| `fog` | Fog |
| `fog-day` | Fog during the day |
| `fog-night` | Fog at night |
| `haze` | Haze |
| `haze-day` | Haze during the day |
| `haze-night` | Haze at night |
| `dust` | Dust in the air |
| `dust-day` | Daytime dust or sand |
| `dust-night` | Nighttime dust or sand |
| `sun-hot` | Intense sunshine / heat stress |

---

## Mostly Clear

Composite condition icons (18 slugs). Each row is one fill SVG at `plugins/weather/icons/fill/{slug}.svg`.

| Slug | Purpose |
|---|---|
| `mostly-clear-day` | Mostly clear sky during the day |
| `mostly-clear-day-drizzle` | Mostly clear sky during the day with light drizzle |
| `mostly-clear-day-fog` | Mostly clear sky during the day with fog |
| `mostly-clear-day-hail` | Mostly clear sky during the day with hail |
| `mostly-clear-day-haze` | Mostly clear sky during the day with haze |
| `mostly-clear-day-rain` | Mostly clear sky during the day with rain |
| `mostly-clear-day-sleet` | Mostly clear sky during the day with sleet (rain and snow mixed) |
| `mostly-clear-day-smoke` | Mostly clear sky during the day with smoke or poor air quality |
| `mostly-clear-day-snow` | Mostly clear sky during the day with snow |
| `mostly-clear-night` | Mostly clear sky at night |
| `mostly-clear-night-drizzle` | Mostly clear sky at night with light drizzle |
| `mostly-clear-night-fog` | Mostly clear sky at night with fog |
| `mostly-clear-night-hail` | Mostly clear sky at night with hail |
| `mostly-clear-night-haze` | Mostly clear sky at night with haze |
| `mostly-clear-night-rain` | Mostly clear sky at night with rain |
| `mostly-clear-night-sleet` | Mostly clear sky at night with sleet (rain and snow mixed) |
| `mostly-clear-night-smoke` | Mostly clear sky at night with smoke or poor air quality |
| `mostly-clear-night-snow` | Mostly clear sky at night with snow |

---

## Partly Cloudy

Composite condition icons (18 slugs). Each row is one fill SVG at `plugins/weather/icons/fill/{slug}.svg`.

| Slug | Purpose |
|---|---|
| `partly-cloudy-day` | Partly cloudy sky during the day |
| `partly-cloudy-day-drizzle` | Partly cloudy sky during the day with light drizzle |
| `partly-cloudy-day-fog` | Partly cloudy sky during the day with fog |
| `partly-cloudy-day-hail` | Partly cloudy sky during the day with hail |
| `partly-cloudy-day-haze` | Partly cloudy sky during the day with haze |
| `partly-cloudy-day-rain` | Partly cloudy sky during the day with rain |
| `partly-cloudy-day-sleet` | Partly cloudy sky during the day with sleet (rain and snow mixed) |
| `partly-cloudy-day-smoke` | Partly cloudy sky during the day with smoke or poor air quality |
| `partly-cloudy-day-snow` | Partly cloudy sky during the day with snow |
| `partly-cloudy-night` | Partly cloudy sky at night |
| `partly-cloudy-night-drizzle` | Partly cloudy sky at night with light drizzle |
| `partly-cloudy-night-fog` | Partly cloudy sky at night with fog |
| `partly-cloudy-night-hail` | Partly cloudy sky at night with hail |
| `partly-cloudy-night-haze` | Partly cloudy sky at night with haze |
| `partly-cloudy-night-rain` | Partly cloudy sky at night with rain |
| `partly-cloudy-night-sleet` | Partly cloudy sky at night with sleet (rain and snow mixed) |
| `partly-cloudy-night-smoke` | Partly cloudy sky at night with smoke or poor air quality |
| `partly-cloudy-night-snow` | Partly cloudy sky at night with snow |

---

## Overcast

Composite condition icons (27 slugs). Each row is one fill SVG at `plugins/weather/icons/fill/{slug}.svg`.

| Slug | Purpose |
|---|---|
| `overcast` | Overcast sky |
| `overcast-drizzle` | Overcast sky with light drizzle |
| `overcast-fog` | Overcast sky with fog |
| `overcast-hail` | Overcast sky with hail |
| `overcast-haze` | Overcast sky with haze |
| `overcast-rain` | Overcast sky with rain |
| `overcast-sleet` | Overcast sky with sleet (rain and snow mixed) |
| `overcast-smoke` | Overcast sky with smoke or poor air quality |
| `overcast-snow` | Overcast sky with snow |
| `overcast-day` | Overcast sky during the day |
| `overcast-day-drizzle` | Overcast sky during the day with light drizzle |
| `overcast-day-fog` | Overcast sky during the day with fog |
| `overcast-day-hail` | Overcast sky during the day with hail |
| `overcast-day-haze` | Overcast sky during the day with haze |
| `overcast-day-rain` | Overcast sky during the day with rain |
| `overcast-day-sleet` | Overcast sky during the day with sleet (rain and snow mixed) |
| `overcast-day-smoke` | Overcast sky during the day with smoke or poor air quality |
| `overcast-day-snow` | Overcast sky during the day with snow |
| `overcast-night` | Overcast sky at night |
| `overcast-night-drizzle` | Overcast sky at night with light drizzle |
| `overcast-night-fog` | Overcast sky at night with fog |
| `overcast-night-hail` | Overcast sky at night with hail |
| `overcast-night-haze` | Overcast sky at night with haze |
| `overcast-night-rain` | Overcast sky at night with rain |
| `overcast-night-sleet` | Overcast sky at night with sleet (rain and snow mixed) |
| `overcast-night-smoke` | Overcast sky at night with smoke or poor air quality |
| `overcast-night-snow` | Overcast sky at night with snow |

---

## Extreme

Composite condition icons (27 slugs). Each row is one fill SVG at `plugins/weather/icons/fill/{slug}.svg`.

| Slug | Purpose |
|---|---|
| `extreme` | Extreme precipitation or storm intensity |
| `extreme-drizzle` | Extreme precipitation or storm intensity with light drizzle |
| `extreme-fog` | Extreme precipitation or storm intensity with fog |
| `extreme-hail` | Extreme precipitation or storm intensity with hail |
| `extreme-haze` | Extreme precipitation or storm intensity with haze |
| `extreme-rain` | Extreme precipitation or storm intensity with rain |
| `extreme-sleet` | Extreme precipitation or storm intensity with sleet (rain and snow mixed) |
| `extreme-smoke` | Extreme precipitation or storm intensity with smoke or poor air quality |
| `extreme-snow` | Extreme precipitation or storm intensity with snow |
| `extreme-day` | Extreme precipitation or storm intensity during the day |
| `extreme-day-drizzle` | Extreme precipitation or storm intensity during the day with light drizzle |
| `extreme-day-fog` | Extreme precipitation or storm intensity during the day with fog |
| `extreme-day-hail` | Extreme precipitation or storm intensity during the day with hail |
| `extreme-day-haze` | Extreme precipitation or storm intensity during the day with haze |
| `extreme-day-rain` | Extreme precipitation or storm intensity during the day with rain |
| `extreme-day-sleet` | Extreme precipitation or storm intensity during the day with sleet (rain and snow mixed) |
| `extreme-day-smoke` | Extreme precipitation or storm intensity during the day with smoke or poor air quality |
| `extreme-day-snow` | Extreme precipitation or storm intensity during the day with snow |
| `extreme-night` | Extreme precipitation or storm intensity at night |
| `extreme-night-drizzle` | Extreme precipitation or storm intensity at night with light drizzle |
| `extreme-night-fog` | Extreme precipitation or storm intensity at night with fog |
| `extreme-night-hail` | Extreme precipitation or storm intensity at night with hail |
| `extreme-night-haze` | Extreme precipitation or storm intensity at night with haze |
| `extreme-night-rain` | Extreme precipitation or storm intensity at night with rain |
| `extreme-night-sleet` | Extreme precipitation or storm intensity at night with sleet (rain and snow mixed) |
| `extreme-night-smoke` | Extreme precipitation or storm intensity at night with smoke or poor air quality |
| `extreme-night-snow` | Extreme precipitation or storm intensity at night with snow |

---

## Thunderstorms

Composite condition icons (99 slugs). Each row is one fill SVG at `plugins/weather/icons/fill/{slug}.svg`.

| Slug | Purpose |
|---|---|
| `thunderstorms` | Thunderstorm |
| `thunderstorms-drizzle` | Thunderstorm with light drizzle |
| `thunderstorms-fog` | Thunderstorm with fog |
| `thunderstorms-hail` | Thunderstorm with hail |
| `thunderstorms-haze` | Thunderstorm with haze |
| `thunderstorms-rain` | Thunderstorm with rain |
| `thunderstorms-sleet` | Thunderstorm with sleet (rain and snow mixed) |
| `thunderstorms-smoke` | Thunderstorm with smoke or poor air quality |
| `thunderstorms-snow` | Thunderstorm with snow |
| `thunderstorms-day` | Thunderstorm during the day |
| `thunderstorms-day-drizzle` | Thunderstorm during the day with light drizzle |
| `thunderstorms-day-fog` | Thunderstorm during the day with fog |
| `thunderstorms-day-hail` | Thunderstorm during the day with hail |
| `thunderstorms-day-haze` | Thunderstorm during the day with haze |
| `thunderstorms-day-rain` | Thunderstorm during the day with rain |
| `thunderstorms-day-sleet` | Thunderstorm during the day with sleet (rain and snow mixed) |
| `thunderstorms-day-smoke` | Thunderstorm during the day with smoke or poor air quality |
| `thunderstorms-day-snow` | Thunderstorm during the day with snow |
| `thunderstorms-night` | Thunderstorm at night |
| `thunderstorms-night-drizzle` | Thunderstorm at night with light drizzle |
| `thunderstorms-night-fog` | Thunderstorm at night with fog |
| `thunderstorms-night-hail` | Thunderstorm at night with hail |
| `thunderstorms-night-haze` | Thunderstorm at night with haze |
| `thunderstorms-night-rain` | Thunderstorm at night with rain |
| `thunderstorms-night-sleet` | Thunderstorm at night with sleet (rain and snow mixed) |
| `thunderstorms-night-smoke` | Thunderstorm at night with smoke or poor air quality |
| `thunderstorms-night-snow` | Thunderstorm at night with snow |
| `thunderstorms-mostly-clear-day` | Thunderstorm with mostly clear during the day |
| `thunderstorms-mostly-clear-day-drizzle` | Thunderstorm with mostly clear during the day with light drizzle |
| `thunderstorms-mostly-clear-day-fog` | Thunderstorm with mostly clear during the day with fog |
| `thunderstorms-mostly-clear-day-hail` | Thunderstorm with mostly clear during the day with hail |
| `thunderstorms-mostly-clear-day-haze` | Thunderstorm with mostly clear during the day with haze |
| `thunderstorms-mostly-clear-day-rain` | Thunderstorm with mostly clear during the day with rain |
| `thunderstorms-mostly-clear-day-sleet` | Thunderstorm with mostly clear during the day with sleet (rain and snow mixed) |
| `thunderstorms-mostly-clear-day-smoke` | Thunderstorm with mostly clear during the day with smoke or poor air quality |
| `thunderstorms-mostly-clear-day-snow` | Thunderstorm with mostly clear during the day with snow |
| `thunderstorms-mostly-clear-night` | Thunderstorm with mostly clear at night |
| `thunderstorms-mostly-clear-night-drizzle` | Thunderstorm with mostly clear at night with light drizzle |
| `thunderstorms-mostly-clear-night-fog` | Thunderstorm with mostly clear at night with fog |
| `thunderstorms-mostly-clear-night-hail` | Thunderstorm with mostly clear at night with hail |
| `thunderstorms-mostly-clear-night-haze` | Thunderstorm with mostly clear at night with haze |
| `thunderstorms-mostly-clear-night-rain` | Thunderstorm with mostly clear at night with rain |
| `thunderstorms-mostly-clear-night-sleet` | Thunderstorm with mostly clear at night with sleet (rain and snow mixed) |
| `thunderstorms-mostly-clear-night-smoke` | Thunderstorm with mostly clear at night with smoke or poor air quality |
| `thunderstorms-mostly-clear-night-snow` | Thunderstorm with mostly clear at night with snow |
| `thunderstorms-overcast` | Thunderstorm with overcast |
| `thunderstorms-overcast-drizzle` | Thunderstorm with overcast and light drizzle |
| `thunderstorms-overcast-fog` | Thunderstorm with overcast and fog |
| `thunderstorms-overcast-hail` | Thunderstorm with overcast and hail |
| `thunderstorms-overcast-haze` | Thunderstorm with overcast and haze |
| `thunderstorms-overcast-rain` | Thunderstorm with overcast and rain |
| `thunderstorms-overcast-sleet` | Thunderstorm with overcast and sleet (rain and snow mixed) |
| `thunderstorms-overcast-smoke` | Thunderstorm with overcast and smoke or poor air quality |
| `thunderstorms-overcast-snow` | Thunderstorm with overcast and snow |
| `thunderstorms-overcast-day` | Thunderstorm with overcast during the day |
| `thunderstorms-overcast-day-drizzle` | Thunderstorm with overcast during the day with light drizzle |
| `thunderstorms-overcast-day-fog` | Thunderstorm with overcast during the day with fog |
| `thunderstorms-overcast-day-hail` | Thunderstorm with overcast during the day with hail |
| `thunderstorms-overcast-day-haze` | Thunderstorm with overcast during the day with haze |
| `thunderstorms-overcast-day-rain` | Thunderstorm with overcast during the day with rain |
| `thunderstorms-overcast-day-sleet` | Thunderstorm with overcast during the day with sleet (rain and snow mixed) |
| `thunderstorms-overcast-day-smoke` | Thunderstorm with overcast during the day with smoke or poor air quality |
| `thunderstorms-overcast-day-snow` | Thunderstorm with overcast during the day with snow |
| `thunderstorms-overcast-night` | Thunderstorm with overcast at night |
| `thunderstorms-overcast-night-drizzle` | Thunderstorm with overcast at night with light drizzle |
| `thunderstorms-overcast-night-fog` | Thunderstorm with overcast at night with fog |
| `thunderstorms-overcast-night-hail` | Thunderstorm with overcast at night with hail |
| `thunderstorms-overcast-night-haze` | Thunderstorm with overcast at night with haze |
| `thunderstorms-overcast-night-rain` | Thunderstorm with overcast at night with rain |
| `thunderstorms-overcast-night-sleet` | Thunderstorm with overcast at night with sleet (rain and snow mixed) |
| `thunderstorms-overcast-night-smoke` | Thunderstorm with overcast at night with smoke or poor air quality |
| `thunderstorms-overcast-night-snow` | Thunderstorm with overcast at night with snow |
| `thunderstorms-extreme` | Thunderstorm with extreme precipitation or storm intensity |
| `thunderstorms-extreme-drizzle` | Thunderstorm with extreme precipitation or storm intensity and light drizzle |
| `thunderstorms-extreme-fog` | Thunderstorm with extreme precipitation or storm intensity and fog |
| `thunderstorms-extreme-hail` | Thunderstorm with extreme precipitation or storm intensity and hail |
| `thunderstorms-extreme-haze` | Thunderstorm with extreme precipitation or storm intensity and haze |
| `thunderstorms-extreme-rain` | Thunderstorm with extreme precipitation or storm intensity and rain |
| `thunderstorms-extreme-sleet` | Thunderstorm with extreme precipitation or storm intensity and sleet (rain and snow mixed) |
| `thunderstorms-extreme-smoke` | Thunderstorm with extreme precipitation or storm intensity and smoke or poor air quality |
| `thunderstorms-extreme-snow` | Thunderstorm with extreme precipitation or storm intensity and snow |
| `thunderstorms-extreme-day` | Thunderstorm with extreme precipitation or storm intensity during the day |
| `thunderstorms-extreme-day-drizzle` | Thunderstorm with extreme precipitation or storm intensity during the day with light drizzle |
| `thunderstorms-extreme-day-fog` | Thunderstorm with extreme precipitation or storm intensity during the day with fog |
| `thunderstorms-extreme-day-hail` | Thunderstorm with extreme precipitation or storm intensity during the day with hail |
| `thunderstorms-extreme-day-haze` | Thunderstorm with extreme precipitation or storm intensity during the day with haze |
| `thunderstorms-extreme-day-rain` | Thunderstorm with extreme precipitation or storm intensity during the day with rain |
| `thunderstorms-extreme-day-sleet` | Thunderstorm with extreme precipitation or storm intensity during the day with sleet (rain and snow mixed) |
| `thunderstorms-extreme-day-smoke` | Thunderstorm with extreme precipitation or storm intensity during the day with smoke or poor air quality |
| `thunderstorms-extreme-day-snow` | Thunderstorm with extreme precipitation or storm intensity during the day with snow |
| `thunderstorms-extreme-night` | Thunderstorm with extreme precipitation or storm intensity at night |
| `thunderstorms-extreme-night-drizzle` | Thunderstorm with extreme precipitation or storm intensity at night with light drizzle |
| `thunderstorms-extreme-night-fog` | Thunderstorm with extreme precipitation or storm intensity at night with fog |
| `thunderstorms-extreme-night-hail` | Thunderstorm with extreme precipitation or storm intensity at night with hail |
| `thunderstorms-extreme-night-haze` | Thunderstorm with extreme precipitation or storm intensity at night with haze |
| `thunderstorms-extreme-night-rain` | Thunderstorm with extreme precipitation or storm intensity at night with rain |
| `thunderstorms-extreme-night-sleet` | Thunderstorm with extreme precipitation or storm intensity at night with sleet (rain and snow mixed) |
| `thunderstorms-extreme-night-smoke` | Thunderstorm with extreme precipitation or storm intensity at night with smoke or poor air quality |
| `thunderstorms-extreme-night-snow` | Thunderstorm with extreme precipitation or storm intensity at night with snow |

---

## Extreme Thunderstorms

Composite condition icons (99 slugs). Each row is one fill SVG at `plugins/weather/icons/fill/{slug}.svg`.

| Slug | Purpose |
|---|---|
| `extreme-thunderstorms` | Severe thunderstorm |
| `extreme-thunderstorms-drizzle` | Severe thunderstorm with light drizzle |
| `extreme-thunderstorms-fog` | Severe thunderstorm with fog |
| `extreme-thunderstorms-hail` | Severe thunderstorm with hail |
| `extreme-thunderstorms-haze` | Severe thunderstorm with haze |
| `extreme-thunderstorms-rain` | Severe thunderstorm with rain |
| `extreme-thunderstorms-sleet` | Severe thunderstorm with sleet (rain and snow mixed) |
| `extreme-thunderstorms-smoke` | Severe thunderstorm with smoke or poor air quality |
| `extreme-thunderstorms-snow` | Severe thunderstorm with snow |
| `extreme-thunderstorms-day` | Severe thunderstorm during the day |
| `extreme-thunderstorms-day-drizzle` | Severe thunderstorm during the day with light drizzle |
| `extreme-thunderstorms-day-fog` | Severe thunderstorm during the day with fog |
| `extreme-thunderstorms-day-hail` | Severe thunderstorm during the day with hail |
| `extreme-thunderstorms-day-haze` | Severe thunderstorm during the day with haze |
| `extreme-thunderstorms-day-rain` | Severe thunderstorm during the day with rain |
| `extreme-thunderstorms-day-sleet` | Severe thunderstorm during the day with sleet (rain and snow mixed) |
| `extreme-thunderstorms-day-smoke` | Severe thunderstorm during the day with smoke or poor air quality |
| `extreme-thunderstorms-day-snow` | Severe thunderstorm during the day with snow |
| `extreme-thunderstorms-night` | Severe thunderstorm at night |
| `extreme-thunderstorms-night-drizzle` | Severe thunderstorm at night with light drizzle |
| `extreme-thunderstorms-night-fog` | Severe thunderstorm at night with fog |
| `extreme-thunderstorms-night-hail` | Severe thunderstorm at night with hail |
| `extreme-thunderstorms-night-haze` | Severe thunderstorm at night with haze |
| `extreme-thunderstorms-night-rain` | Severe thunderstorm at night with rain |
| `extreme-thunderstorms-night-sleet` | Severe thunderstorm at night with sleet (rain and snow mixed) |
| `extreme-thunderstorms-night-smoke` | Severe thunderstorm at night with smoke or poor air quality |
| `extreme-thunderstorms-night-snow` | Severe thunderstorm at night with snow |
| `extreme-thunderstorms-mostly-clear-day` | Severe thunderstorm with mostly clear sky during the day |
| `extreme-thunderstorms-mostly-clear-day-drizzle` | Severe thunderstorm with mostly clear sky during the day with light drizzle |
| `extreme-thunderstorms-mostly-clear-day-fog` | Severe thunderstorm with mostly clear sky during the day with fog |
| `extreme-thunderstorms-mostly-clear-day-hail` | Severe thunderstorm with mostly clear sky during the day with hail |
| `extreme-thunderstorms-mostly-clear-day-haze` | Severe thunderstorm with mostly clear sky during the day with haze |
| `extreme-thunderstorms-mostly-clear-day-rain` | Severe thunderstorm with mostly clear sky during the day with rain |
| `extreme-thunderstorms-mostly-clear-day-sleet` | Severe thunderstorm with mostly clear sky during the day with sleet (rain and snow mixed) |
| `extreme-thunderstorms-mostly-clear-day-smoke` | Severe thunderstorm with mostly clear sky during the day with smoke or poor air quality |
| `extreme-thunderstorms-mostly-clear-day-snow` | Severe thunderstorm with mostly clear sky during the day with snow |
| `extreme-thunderstorms-mostly-clear-night` | Severe thunderstorm with mostly clear sky at night |
| `extreme-thunderstorms-mostly-clear-night-drizzle` | Severe thunderstorm with mostly clear sky at night with light drizzle |
| `extreme-thunderstorms-mostly-clear-night-fog` | Severe thunderstorm with mostly clear sky at night with fog |
| `extreme-thunderstorms-mostly-clear-night-hail` | Severe thunderstorm with mostly clear sky at night with hail |
| `extreme-thunderstorms-mostly-clear-night-haze` | Severe thunderstorm with mostly clear sky at night with haze |
| `extreme-thunderstorms-mostly-clear-night-rain` | Severe thunderstorm with mostly clear sky at night with rain |
| `extreme-thunderstorms-mostly-clear-night-sleet` | Severe thunderstorm with mostly clear sky at night with sleet (rain and snow mixed) |
| `extreme-thunderstorms-mostly-clear-night-smoke` | Severe thunderstorm with mostly clear sky at night with smoke or poor air quality |
| `extreme-thunderstorms-mostly-clear-night-snow` | Severe thunderstorm with mostly clear sky at night with snow |
| `extreme-thunderstorms-overcast` | Severe thunderstorm with overcast sky |
| `extreme-thunderstorms-overcast-drizzle` | Severe thunderstorm with overcast sky and light drizzle |
| `extreme-thunderstorms-overcast-fog` | Severe thunderstorm with overcast sky and fog |
| `extreme-thunderstorms-overcast-hail` | Severe thunderstorm with overcast sky and hail |
| `extreme-thunderstorms-overcast-haze` | Severe thunderstorm with overcast sky and haze |
| `extreme-thunderstorms-overcast-rain` | Severe thunderstorm with overcast sky and rain |
| `extreme-thunderstorms-overcast-sleet` | Severe thunderstorm with overcast sky and sleet (rain and snow mixed) |
| `extreme-thunderstorms-overcast-smoke` | Severe thunderstorm with overcast sky and smoke or poor air quality |
| `extreme-thunderstorms-overcast-snow` | Severe thunderstorm with overcast sky and snow |
| `extreme-thunderstorms-overcast-day` | Severe thunderstorm with overcast sky during the day |
| `extreme-thunderstorms-overcast-day-drizzle` | Severe thunderstorm with overcast sky during the day with light drizzle |
| `extreme-thunderstorms-overcast-day-fog` | Severe thunderstorm with overcast sky during the day with fog |
| `extreme-thunderstorms-overcast-day-hail` | Severe thunderstorm with overcast sky during the day with hail |
| `extreme-thunderstorms-overcast-day-haze` | Severe thunderstorm with overcast sky during the day with haze |
| `extreme-thunderstorms-overcast-day-rain` | Severe thunderstorm with overcast sky during the day with rain |
| `extreme-thunderstorms-overcast-day-sleet` | Severe thunderstorm with overcast sky during the day with sleet (rain and snow mixed) |
| `extreme-thunderstorms-overcast-day-smoke` | Severe thunderstorm with overcast sky during the day with smoke or poor air quality |
| `extreme-thunderstorms-overcast-day-snow` | Severe thunderstorm with overcast sky during the day with snow |
| `extreme-thunderstorms-overcast-night` | Severe thunderstorm with overcast sky at night |
| `extreme-thunderstorms-overcast-night-drizzle` | Severe thunderstorm with overcast sky at night with light drizzle |
| `extreme-thunderstorms-overcast-night-fog` | Severe thunderstorm with overcast sky at night with fog |
| `extreme-thunderstorms-overcast-night-hail` | Severe thunderstorm with overcast sky at night with hail |
| `extreme-thunderstorms-overcast-night-haze` | Severe thunderstorm with overcast sky at night with haze |
| `extreme-thunderstorms-overcast-night-rain` | Severe thunderstorm with overcast sky at night with rain |
| `extreme-thunderstorms-overcast-night-sleet` | Severe thunderstorm with overcast sky at night with sleet (rain and snow mixed) |
| `extreme-thunderstorms-overcast-night-smoke` | Severe thunderstorm with overcast sky at night with smoke or poor air quality |
| `extreme-thunderstorms-overcast-night-snow` | Severe thunderstorm with overcast sky at night with snow |
| `extreme-thunderstorms-extreme` | Severe thunderstorm with extreme precipitation or storm intensity |
| `extreme-thunderstorms-extreme-drizzle` | Severe thunderstorm with extreme precipitation or storm intensity and light drizzle |
| `extreme-thunderstorms-extreme-fog` | Severe thunderstorm with extreme precipitation or storm intensity and fog |
| `extreme-thunderstorms-extreme-hail` | Severe thunderstorm with extreme precipitation or storm intensity and hail |
| `extreme-thunderstorms-extreme-haze` | Severe thunderstorm with extreme precipitation or storm intensity and haze |
| `extreme-thunderstorms-extreme-rain` | Severe thunderstorm with extreme precipitation or storm intensity and rain |
| `extreme-thunderstorms-extreme-sleet` | Severe thunderstorm with extreme precipitation or storm intensity and sleet (rain and snow mixed) |
| `extreme-thunderstorms-extreme-smoke` | Severe thunderstorm with extreme precipitation or storm intensity and smoke or poor air quality |
| `extreme-thunderstorms-extreme-snow` | Severe thunderstorm with extreme precipitation or storm intensity and snow |
| `extreme-thunderstorms-extreme-day` | Severe thunderstorm with extreme precipitation or storm intensity during the day |
| `extreme-thunderstorms-extreme-day-drizzle` | Severe thunderstorm with extreme precipitation or storm intensity during the day with light drizzle |
| `extreme-thunderstorms-extreme-day-fog` | Severe thunderstorm with extreme precipitation or storm intensity during the day with fog |
| `extreme-thunderstorms-extreme-day-hail` | Severe thunderstorm with extreme precipitation or storm intensity during the day with hail |
| `extreme-thunderstorms-extreme-day-haze` | Severe thunderstorm with extreme precipitation or storm intensity during the day with haze |
| `extreme-thunderstorms-extreme-day-rain` | Severe thunderstorm with extreme precipitation or storm intensity during the day with rain |
| `extreme-thunderstorms-extreme-day-sleet` | Severe thunderstorm with extreme precipitation or storm intensity during the day with sleet (rain and snow mixed) |
| `extreme-thunderstorms-extreme-day-smoke` | Severe thunderstorm with extreme precipitation or storm intensity during the day with smoke or poor air quality |
| `extreme-thunderstorms-extreme-day-snow` | Severe thunderstorm with extreme precipitation or storm intensity during the day with snow |
| `extreme-thunderstorms-extreme-night` | Severe thunderstorm with extreme precipitation or storm intensity at night |
| `extreme-thunderstorms-extreme-night-drizzle` | Severe thunderstorm with extreme precipitation or storm intensity at night with light drizzle |
| `extreme-thunderstorms-extreme-night-fog` | Severe thunderstorm with extreme precipitation or storm intensity at night with fog |
| `extreme-thunderstorms-extreme-night-hail` | Severe thunderstorm with extreme precipitation or storm intensity at night with hail |
| `extreme-thunderstorms-extreme-night-haze` | Severe thunderstorm with extreme precipitation or storm intensity at night with haze |
| `extreme-thunderstorms-extreme-night-rain` | Severe thunderstorm with extreme precipitation or storm intensity at night with rain |
| `extreme-thunderstorms-extreme-night-sleet` | Severe thunderstorm with extreme precipitation or storm intensity at night with sleet (rain and snow mixed) |
| `extreme-thunderstorms-extreme-night-smoke` | Severe thunderstorm with extreme precipitation or storm intensity at night with smoke or poor air quality |
| `extreme-thunderstorms-extreme-night-snow` | Severe thunderstorm with extreme precipitation or storm intensity at night with snow |

---

## Alarms

38 icons. File path: `plugins/weather/icons/fill/{slug}.svg`.

| Slug | Purpose |
|---|---|
| `code-black` | Highest-severity alert code (black) |
| `code-green` | No significant weather threat (green) |
| `code-orange` | Significant weather expected (orange) |
| `code-purple` | Extreme weather possible (purple) |
| `code-red` | Severe weather warning (red) |
| `code-yellow` | Weather advisory (yellow) |
| `weather-alert` | Active weather alert (generic) |
| `weather-alert-day` | Weather alert during the day |
| `weather-alert-night` | Weather alert at night |
| `cyclone` | Tropical cyclone |
| `cyclone-alert` | Tropical cyclone warning |
| `hurricane` | Hurricane |
| `hurricane-alert` | Hurricane warning |
| `tornado` | Tornado |
| `tornado-alert` | Tornado warning |
| `waterspout` | Waterspout |
| `waterspout-alert` | Waterspout warning |
| `avalanche-danger-alert` | Avalanche danger warning |
| `falling-rocks-alert` | Falling rocks / debris hazard |
| `fire-alert` | Wildfire or fire weather alert |
| `water-alert` | Flood or water-related alert |
| `volcano` | Volcano (inactive or generic) |
| `volcano-alert` | Volcano activity alert |
| `volcano-eruption` | Volcanic eruption |
| `volcano-eruption-alert` | Eruption warning |
| `flag-cold-wave` | Cold wave warning flag |
| `flag-fair` | Fair weather flag |
| `flag-gale-warning` | Gale warning flag |
| `flag-hurricane-warning` | Hurricane warning flag |
| `flag-local-rain` | Local rain expected flag |
| `flag-ne-storm-warning` | Northeast storm warning flag |
| `flag-nw-storm-warning` | Northwest storm warning flag |
| `flag-rain` | Rain expected flag |
| `flag-se-storm-warning` | Southeast storm warning flag |
| `flag-small-craft-advisory` | Small craft advisory flag |
| `flag-storm-warning` | Storm warning flag |
| `flag-sw-storm-warning` | Southwest storm warning flag |
| `flag-temperature-change` | Sharp temperature change expected |

---

## Astronomical

16 icons. File path: `plugins/weather/icons/fill/{slug}.svg`.

| Slug | Purpose |
|---|---|
| `horizon` | Horizon line (generic astronomical) |
| `sunrise` | Sunrise |
| `sunset` | Sunset |
| `moonrise` | Moonrise |
| `moonset` | Moonset |
| `moon-new` | New moon |
| `moon-waxing-crescent` | Waxing crescent moon |
| `moon-first-quarter` | First quarter moon |
| `moon-waxing-gibbous` | Waxing gibbous moon |
| `moon-full` | Full moon |
| `moon-waning-gibbous` | Waning gibbous moon |
| `moon-last-quarter` | Last quarter moon |
| `moon-waning-crescent` | Waning crescent moon |
| `falling-stars` | Meteor shower / falling stars |
| `solar-eclipse` | Solar eclipse |
| `starry-night` | Clear starry night |

---

## Miscellaneous

47 icons. File path: `plugins/weather/icons/fill/{slug}.svg`.

| Slug | Purpose |
|---|---|
| `barometer` | Atmospheric pressure (generic barometer) |
| `barometer-low` | Low pressure system |
| `barometer-moderate` | Moderate pressure |
| `barometer-high` | High pressure system |
| `barometer-very-high` | Very high pressure |
| `barometer-extreme` | Extremely high or low pressure (alert level) |
| `pressure-high` | Rising pressure trend |
| `pressure-low` | Falling pressure trend |
| `pressure-high-alt` | High pressure (alternate icon) |
| `pressure-low-alt` | Low pressure (alternate icon) |
| `compass` | Compass rose (no direction highlighted) |
| `compass-n` | North |
| `compass-ne` | Northeast |
| `compass-e` | East |
| `compass-se` | Southeast |
| `compass-s` | South |
| `compass-sw` | Southwest |
| `compass-w` | West |
| `compass-nw` | Northwest |
| `celsius` | Temperature in degrees Celsius |
| `fahrenheit` | Temperature in degrees Fahrenheit |
| `kelvin` | Temperature in kelvin |
| `not-available` | Measurement or forecast unavailable |
| `lightning-bolt` | Lightning (single strike) |
| `lightning-bolts` | Lightning (multiple strikes) |
| `rainbow` | Rainbow |
| `rainbow-clear` | Rainbow with clear sky |
| `rainbow-cloud` | Rainbow with cloud |
| `humidity` | Relative humidity |
| `raindrop` | Single raindrop / light precipitation |
| `raindrops` | Multiple raindrops / precipitation |
| `raindrop-measure` | Measured rainfall amount |
| `water` | Water / liquid precipitation generic |
| `water-tide-high` | High tide |
| `water-tide-low` | Low tide |
| `beanie` | Cold weather — wear a hat |
| `glove` | Cold weather — wear gloves |
| `snowflake` | Snowflake / freezing conditions |
| `snowman` | Snow on the ground / playful snow conditions |
| `smoke-particles` | Airborne smoke particles |
| `star` | Star (generic) |
| `soil-moisture` | Soil moisture level |
| `soil-temperature` | Soil temperature |
| `umbrella` | Umbrella recommended (rain) |
| `umbrella-closed` | No umbrella needed / dry |
| `umbrella-wind` | Umbrella impractical — windy rain |
| `umbrella-wind-alt` | Wind and rain (alternate) |

---

## Solar and Power

18 icons. File path: `plugins/weather/icons/fill/{slug}.svg`.

| Slug | Purpose |
|---|---|
| `solar-panel` | Solar panel output (generic) |
| `solar-panel-day` | Solar generation during daylight |
| `solar-panel-night` | No solar generation at night |
| `solar-panel-cloudy` | Reduced solar output under cloud |
| `solar-panel-low` | Low solar yield |
| `solar-panel-medium` | Moderate solar yield |
| `solar-panel-high` | High solar yield |
| `solar-panel-alert` | Solar system alert |
| `solar-panel-battery-empty` | Storage empty |
| `solar-panel-battery-low` | Storage low |
| `solar-panel-battery-half` | Storage about half full |
| `solar-panel-battery-full` | Storage full |
| `solar-panel-battery-charging` | Battery charging from solar |
| `grid-eco` | Grid power — eco / low-carbon period |
| `grid-solar` | Grid fed by solar |
| `grid-price-low` | Low electricity price period |
| `grid-import` | Importing power from grid |
| `grid-export` | Exporting power to grid |

---

## Thermometer

17 icons. File path: `plugins/weather/icons/fill/{slug}.svg`.

| Slug | Purpose |
|---|---|
| `thermometer` | Air temperature |
| `thermometer-alert` | Temperature alert (extreme heat or cold) |
| `thermometer-celsius` | Temperature with Celsius scale |
| `thermometer-colder` | Temperature falling / getting colder |
| `thermometer-fahrenheit` | Temperature with Fahrenheit scale |
| `thermometer-warmer` | Temperature rising / getting warmer |
| `thermometer-glass` | Liquid-in-glass thermometer |
| `thermometer-glass-alert` | Liquid thermometer — alert level |
| `thermometer-glass-celsius` | Glass thermometer (Celsius) |
| `thermometer-glass-fahrenheit` | Glass thermometer (Fahrenheit) |
| `thermometer-mercury` | Mercury-style thermometer |
| `thermometer-mercury-cold` | Very cold temperature |
| `thermometer-moon` | Overnight low temperature |
| `thermometer-raindrop` | Wet-bulb or rain-cooled temperature feel |
| `thermometer-snow` | Freezing / snow-related temperature |
| `thermometer-sun` | Daytime high / heat index feel |
| `thermometer-water` | Water temperature |

---

## Time

8 icons. File path: `plugins/weather/icons/fill/{slug}.svg`.

| Slug | Purpose |
|---|---|
| `time-morning` | Morning (roughly sunrise to mid-morning) |
| `time-late-morning` | Late morning |
| `time-afternoon` | Afternoon |
| `time-late-afternoon` | Late afternoon |
| `time-evening` | Evening |
| `time-late-evening` | Late evening |
| `time-night` | Night |
| `time-late-night` | Late night / small hours |

---

## Pollen

18 icons. File path: `plugins/weather/icons/fill/{slug}.svg`.

| Slug | Purpose |
|---|---|
| `pollen` | Pollen (generic) |
| `pollen-flower` | Flower pollen |
| `pollen-grass` | Grass pollen |
| `pollen-tree` | Tree pollen |
| `pollen-tree-fir` | Fir / conifer pollen |
| `pollen-weed` | Weed pollen |
| `pollen-grass-low` | Grass pollen — low level |
| `pollen-grass-moderate` | Grass pollen — moderate |
| `pollen-grass-high` | Grass pollen — high |
| `pollen-grass-very-high` | Grass pollen — very high |
| `pollen-tree-low` | Tree pollen — low |
| `pollen-tree-moderate` | Tree pollen — moderate |
| `pollen-tree-high` | Tree pollen — high |
| `pollen-tree-very-high` | Tree pollen — very high |
| `pollen-weed-low` | Weed pollen — low |
| `pollen-weed-moderate` | Weed pollen — moderate |
| `pollen-weed-high` | Weed pollen — high |
| `pollen-weed-very-high` | Weed pollen — very high |

---

## UV

14 icons. File path: `plugins/weather/icons/fill/{slug}.svg`.

| Slug | Purpose |
|---|---|
| `uv-index` | UV index (generic) |
| `uv-index-alert` | UV index alert — high exposure risk |
| `uv-index-1` | UV index 1 (low) |
| `uv-index-2` | UV index 2 (low) |
| `uv-index-3` | UV index 3 (moderate) |
| `uv-index-4` | UV index 4 (moderate) |
| `uv-index-5` | UV index 5 (moderate) |
| `uv-index-6` | UV index 6 (high) |
| `uv-index-7` | UV index 7 (high) |
| `uv-index-8` | UV index 8 (very high) |
| `uv-index-9` | UV index 9 (very high) |
| `uv-index-10` | UV index 10 (very high) |
| `uv-index-11` | UV index 11 (extreme) |
| `uv-index-11-plus` | UV index 11+ (extreme) |

---

## Wind

33 icons. File path: `plugins/weather/icons/fill/{slug}.svg`.

| Slug | Purpose |
|---|---|
| `wind` | Wind (generic) |
| `wind-alert` | High wind alert |
| `wind-beaufort-0` | Wind force 0 on the Beaufort scale (Calm (0–1 kn)) |
| `wind-beaufort-1` | Wind force 1 on the Beaufort scale (Light air (1–3 kn)) |
| `wind-beaufort-2` | Wind force 2 on the Beaufort scale (Light breeze (4–6 kn)) |
| `wind-beaufort-3` | Wind force 3 on the Beaufort scale (Gentle breeze (7–10 kn)) |
| `wind-beaufort-4` | Wind force 4 on the Beaufort scale (Moderate breeze (11–16 kn)) |
| `wind-beaufort-5` | Wind force 5 on the Beaufort scale (Fresh breeze (17–21 kn)) |
| `wind-beaufort-6` | Wind force 6 on the Beaufort scale (Strong breeze (22–27 kn)) |
| `wind-beaufort-7` | Wind force 7 on the Beaufort scale (Near gale (28–33 kn)) |
| `wind-beaufort-8` | Wind force 8 on the Beaufort scale (Gale (34–40 kn)) |
| `wind-beaufort-9` | Wind force 9 on the Beaufort scale (Strong gale (41–47 kn)) |
| `wind-beaufort-10` | Wind force 10 on the Beaufort scale (Storm (48–55 kn)) |
| `wind-beaufort-11` | Wind force 11 on the Beaufort scale (Violent storm (56–63 kn)) |
| `wind-beaufort-12` | Wind force 12 on the Beaufort scale (Hurricane force (64+ kn)) |
| `wind-direction-n` | Wind from the north |
| `wind-direction-ne` | Wind from the northeast |
| `wind-direction-e` | Wind from the east |
| `wind-direction-se` | Wind from the southeast |
| `wind-direction-s` | Wind from the south |
| `wind-direction-sw` | Wind from the southwest |
| `wind-direction-w` | Wind from the west |
| `wind-direction-nw` | Wind from the northwest |
| `wind-dust` | Wind-blown dust or sand |
| `wind-onshore` | Onshore wind (sea to land) |
| `wind-offshore` | Offshore wind (land to sea) |
| `wind-snow` | Wind-driven snow / blizzard conditions |
| `wind-spinner` | Wind vane / variable wind |
| `windmill` | Windmill (wind energy metaphor) |
| `windsock` | Wind strength (generic windsock) |
| `windsock-weak` | Weak wind |
| `windsock-moderate` | Moderate wind |
| `windsock-calm` | Calm wind |

---

## See also

| Doc | Topic |
|---|---|
| [`weather-plugin.md`](weather-plugin.md) | Plugin spec and asset layout |
| [`icons.md`](../icons.md#meteocons-weather-icons-mit--plugin-assets) | MIT licensing |
| [`plugins.md`](../plugins.md) | Plugin host overview |
