#!/usr/bin/env python3
"""Generate docs/plugins/weather-icons-reference.md from manifest.json."""
from __future__ import annotations

import json
import re
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
MANIFEST = REPO / "plugins/weather/icons/manifest.json"
OUT = REPO / "docs/plugins/weather-icons-reference.md"
FILL = REPO / "plugins/weather/icons/fill"

PRECIP = {
    "drizzle": "light drizzle",
    "rain": "rain",
    "sleet": "sleet (rain and snow mixed)",
    "snow": "snow",
    "hail": "hail",
    "fog": "fog",
    "haze": "haze",
    "smoke": "smoke or poor air quality",
}

SKY = {
    "clear-day": "Clear sky during the day",
    "clear-night": "Clear sky at night",
    "cloudy": "Overcast or fully cloudy",
    "cloud-down": "Cloud base lowering / thickening cloud",
    "cloud-up": "Cloud base rising / thinning cloud",
    "sun-hot": "Intense sunshine / heat stress",
    "mist": "Mist",
    "fog": "Fog",
    "fog-day": "Fog during the day",
    "fog-night": "Fog at night",
    "haze": "Haze",
    "haze-day": "Haze during the day",
    "haze-night": "Haze at night",
    "dust": "Dust in the air",
    "dust-day": "Daytime dust or sand",
    "dust-night": "Nighttime dust or sand",
    "drizzle": "Drizzle",
    "rain": "Rain",
    "sleet": "Sleet",
    "snow": "Snow",
    "hail": "Hail",
    "smoke": "Smoke or haze from fires",
}

SPECIAL: dict[str, str] = {
    "barometer": "Atmospheric pressure (generic barometer)",
    "barometer-low": "Low pressure system",
    "barometer-moderate": "Moderate pressure",
    "barometer-high": "High pressure system",
    "barometer-very-high": "Very high pressure",
    "barometer-extreme": "Extremely high or low pressure (alert level)",
    "pressure-high": "Rising pressure trend",
    "pressure-low": "Falling pressure trend",
    "pressure-high-alt": "High pressure (alternate icon)",
    "pressure-low-alt": "Low pressure (alternate icon)",
    "compass": "Compass rose (no direction highlighted)",
    "compass-n": "North",
    "compass-ne": "Northeast",
    "compass-e": "East",
    "compass-se": "Southeast",
    "compass-s": "South",
    "compass-sw": "Southwest",
    "compass-w": "West",
    "compass-nw": "Northwest",
    "celsius": "Temperature in degrees Celsius",
    "fahrenheit": "Temperature in degrees Fahrenheit",
    "kelvin": "Temperature in kelvin",
    "not-available": "Measurement or forecast unavailable",
    "lightning-bolt": "Lightning (single strike)",
    "lightning-bolts": "Lightning (multiple strikes)",
    "rainbow": "Rainbow",
    "rainbow-clear": "Rainbow with clear sky",
    "rainbow-cloud": "Rainbow with cloud",
    "humidity": "Relative humidity",
    "raindrop": "Single raindrop / light precipitation",
    "raindrops": "Multiple raindrops / precipitation",
    "raindrop-measure": "Measured rainfall amount",
    "water": "Water / liquid precipitation generic",
    "water-tide-high": "High tide",
    "water-tide-low": "Low tide",
    "beanie": "Cold weather — wear a hat",
    "glove": "Cold weather — wear gloves",
    "snowflake": "Snowflake / freezing conditions",
    "snowman": "Snow on the ground / playful snow conditions",
    "smoke-particles": "Airborne smoke particles",
    "star": "Star (generic)",
    "soil-moisture": "Soil moisture level",
    "soil-temperature": "Soil temperature",
    "umbrella": "Umbrella recommended (rain)",
    "umbrella-closed": "No umbrella needed / dry",
    "umbrella-wind": "Umbrella impractical — windy rain",
    "umbrella-wind-alt": "Wind and rain (alternate)",
    "horizon": "Horizon line (generic astronomical)",
    "sunrise": "Sunrise",
    "sunset": "Sunset",
    "moonrise": "Moonrise",
    "moonset": "Moonset",
    "moon-new": "New moon",
    "moon-waxing-crescent": "Waxing crescent moon",
    "moon-first-quarter": "First quarter moon",
    "moon-waxing-gibbous": "Waxing gibbous moon",
    "moon-full": "Full moon",
    "moon-waning-gibbous": "Waning gibbous moon",
    "moon-last-quarter": "Last quarter moon",
    "moon-waning-crescent": "Waning crescent moon",
    "falling-stars": "Meteor shower / falling stars",
    "solar-eclipse": "Solar eclipse",
    "starry-night": "Clear starry night",
    "thermometer": "Air temperature",
    "thermometer-alert": "Temperature alert (extreme heat or cold)",
    "thermometer-celsius": "Temperature with Celsius scale",
    "thermometer-fahrenheit": "Temperature with Fahrenheit scale",
    "thermometer-colder": "Temperature falling / getting colder",
    "thermometer-warmer": "Temperature rising / getting warmer",
    "thermometer-glass": "Liquid-in-glass thermometer",
    "thermometer-glass-alert": "Liquid thermometer — alert level",
    "thermometer-glass-celsius": "Glass thermometer (Celsius)",
    "thermometer-glass-fahrenheit": "Glass thermometer (Fahrenheit)",
    "thermometer-mercury": "Mercury-style thermometer",
    "thermometer-mercury-cold": "Very cold temperature",
    "thermometer-moon": "Overnight low temperature",
    "thermometer-raindrop": "Wet-bulb or rain-cooled temperature feel",
    "thermometer-snow": "Freezing / snow-related temperature",
    "thermometer-sun": "Daytime high / heat index feel",
    "thermometer-water": "Water temperature",
    "time-morning": "Morning (roughly sunrise to mid-morning)",
    "time-late-morning": "Late morning",
    "time-afternoon": "Afternoon",
    "time-late-afternoon": "Late afternoon",
    "time-evening": "Evening",
    "time-late-evening": "Late evening",
    "time-night": "Night",
    "time-late-night": "Late night / small hours",
    "pollen": "Pollen (generic)",
    "pollen-flower": "Flower pollen",
    "pollen-grass": "Grass pollen",
    "pollen-tree": "Tree pollen",
    "pollen-tree-fir": "Fir / conifer pollen",
    "pollen-weed": "Weed pollen",
    "pollen-grass-low": "Grass pollen — low level",
    "pollen-grass-moderate": "Grass pollen — moderate",
    "pollen-grass-high": "Grass pollen — high",
    "pollen-grass-very-high": "Grass pollen — very high",
    "pollen-tree-low": "Tree pollen — low",
    "pollen-tree-moderate": "Tree pollen — moderate",
    "pollen-tree-high": "Tree pollen — high",
    "pollen-tree-very-high": "Tree pollen — very high",
    "pollen-weed-low": "Weed pollen — low",
    "pollen-weed-moderate": "Weed pollen — moderate",
    "pollen-weed-high": "Weed pollen — high",
    "pollen-weed-very-high": "Weed pollen — very high",
    "uv-index": "UV index (generic)",
    "uv-index-alert": "UV index alert — high exposure risk",
    "uv-index-1": "UV index 1 (low)",
    "uv-index-2": "UV index 2 (low)",
    "uv-index-3": "UV index 3 (moderate)",
    "uv-index-4": "UV index 4 (moderate)",
    "uv-index-5": "UV index 5 (moderate)",
    "uv-index-6": "UV index 6 (high)",
    "uv-index-7": "UV index 7 (high)",
    "uv-index-8": "UV index 8 (very high)",
    "uv-index-9": "UV index 9 (very high)",
    "uv-index-10": "UV index 10 (very high)",
    "uv-index-11": "UV index 11 (extreme)",
    "uv-index-11-plus": "UV index 11+ (extreme)",
    "wind": "Wind (generic)",
    "wind-alert": "High wind alert",
    "wind-dust": "Wind-blown dust or sand",
    "wind-onshore": "Onshore wind (sea to land)",
    "wind-offshore": "Offshore wind (land to sea)",
    "wind-snow": "Wind-driven snow / blizzard conditions",
    "wind-spinner": "Wind vane / variable wind",
    "windmill": "Windmill (wind energy metaphor)",
    "windsock": "Wind strength (generic windsock)",
    "windsock-calm": "Calm wind",
    "windsock-weak": "Weak wind",
    "windsock-moderate": "Moderate wind",
    "code-black": "Highest-severity alert code (black)",
    "code-green": "No significant weather threat (green)",
    "code-orange": "Significant weather expected (orange)",
    "code-purple": "Extreme weather possible (purple)",
    "code-red": "Severe weather warning (red)",
    "code-yellow": "Weather advisory (yellow)",
    "weather-alert": "Active weather alert (generic)",
    "weather-alert-day": "Weather alert during the day",
    "weather-alert-night": "Weather alert at night",
    "cyclone": "Tropical cyclone",
    "cyclone-alert": "Tropical cyclone warning",
    "hurricane": "Hurricane",
    "hurricane-alert": "Hurricane warning",
    "tornado": "Tornado",
    "tornado-alert": "Tornado warning",
    "waterspout": "Waterspout",
    "waterspout-alert": "Waterspout warning",
    "avalanche-danger-alert": "Avalanche danger warning",
    "falling-rocks-alert": "Falling rocks / debris hazard",
    "fire-alert": "Wildfire or fire weather alert",
    "water-alert": "Flood or water-related alert",
    "volcano": "Volcano (inactive or generic)",
    "volcano-alert": "Volcano activity alert",
    "volcano-eruption": "Volcanic eruption",
    "volcano-eruption-alert": "Eruption warning",
    "flag-cold-wave": "Cold wave warning flag",
    "flag-fair": "Fair weather flag",
    "flag-gale-warning": "Gale warning flag",
    "flag-hurricane-warning": "Hurricane warning flag",
    "flag-local-rain": "Local rain expected flag",
    "flag-ne-storm-warning": "Northeast storm warning flag",
    "flag-nw-storm-warning": "Northwest storm warning flag",
    "flag-rain": "Rain expected flag",
    "flag-se-storm-warning": "Southeast storm warning flag",
    "flag-small-craft-advisory": "Small craft advisory flag",
    "flag-storm-warning": "Storm warning flag",
    "flag-sw-storm-warning": "Southwest storm warning flag",
    "flag-temperature-change": "Sharp temperature change expected",
    "solar-panel": "Solar panel output (generic)",
    "solar-panel-day": "Solar generation during daylight",
    "solar-panel-night": "No solar generation at night",
    "solar-panel-cloudy": "Reduced solar output under cloud",
    "solar-panel-low": "Low solar yield",
    "solar-panel-medium": "Moderate solar yield",
    "solar-panel-high": "High solar yield",
    "solar-panel-alert": "Solar system alert",
    "solar-panel-battery-empty": "Storage empty",
    "solar-panel-battery-low": "Storage low",
    "solar-panel-battery-half": "Storage about half full",
    "solar-panel-battery-full": "Storage full",
    "solar-panel-battery-charging": "Battery charging from solar",
    "grid-eco": "Grid power — eco / low-carbon period",
    "grid-solar": "Grid fed by solar",
    "grid-price-low": "Low electricity price period",
    "grid-import": "Importing power from grid",
    "grid-export": "Exporting power to grid",
}

BEAUFORT = {
    0: "Calm (0–1 kn)",
    1: "Light air (1–3 kn)",
    2: "Light breeze (4–6 kn)",
    3: "Gentle breeze (7–10 kn)",
    4: "Moderate breeze (11–16 kn)",
    5: "Fresh breeze (17–21 kn)",
    6: "Strong breeze (22–27 kn)",
    7: "Near gale (28–33 kn)",
    8: "Gale (34–40 kn)",
    9: "Strong gale (41–47 kn)",
    10: "Storm (48–55 kn)",
    11: "Violent storm (56–63 kn)",
    12: "Hurricane force (64+ kn)",
}

WIND_DIR = {
    "n": "north",
    "ne": "northeast",
    "e": "east",
    "se": "southeast",
    "s": "south",
    "sw": "southwest",
    "w": "west",
    "nw": "northwest",
}

SKY_TEMPLATES = {
    "mostly-clear": "Mostly clear sky",
    "partly-cloudy": "Partly cloudy sky",
    "overcast": "Overcast sky",
    "extreme": "Extreme precipitation or storm intensity",
    "thunderstorms": "Thunderstorm",
    "extreme-thunderstorms": "Severe thunderstorm",
}


def _parse_daynight_and_precip(tail: str) -> tuple[str, str, str]:
    """Return (daynight phrase, precip key or '', remaining sky tokens)."""
    daynight = ""
    precip = ""
    rest = tail

    for key in sorted(PRECIP, key=len, reverse=True):
        if rest == key:
            return daynight, key, ""
        if rest.endswith("-" + key):
            precip = key
            rest = rest[: -(len(key) + 1)]
            break

    if rest.endswith("-day"):
        daynight = " during the day"
        rest = rest[: -len("-day")]
    elif rest.endswith("-night"):
        daynight = " at night"
        rest = rest[: -len("-night")]
    elif rest == "day":
        daynight = " during the day"
        rest = ""
    elif rest == "night":
        daynight = " at night"
        rest = ""

    return daynight, precip, rest


def _describe_composite(base: str, rest: str) -> str:
    daynight, precip, sky_rest = _parse_daynight_and_precip(rest)
    sky_phrase = ""
    if sky_rest:
        sky_phrase = SKY_TEMPLATES.get(sky_rest, sky_rest.replace("-", " "))
        if sky_phrase.endswith(" sky") and base.lower().startswith("thunder"):
            sky_phrase = sky_phrase[:-4]

    if sky_phrase and daynight and precip:
        return f"{base} with {sky_phrase.lower()}{daynight} with {PRECIP[precip]}"
    if sky_phrase and daynight:
        return f"{base} with {sky_phrase.lower()}{daynight}"
    if sky_phrase and precip:
        return f"{base} with {sky_phrase.lower()} and {PRECIP[precip]}"
    if daynight and precip:
        return f"{base}{daynight} with {PRECIP[precip]}"
    if precip:
        return f"{base} with {PRECIP[precip]}"
    if daynight:
        return f"{base}{daynight}"
    if sky_phrase:
        return f"{base} with {sky_phrase.lower()}"
    return base


def describe_slug(slug: str) -> str:
    if slug in SPECIAL:
        return SPECIAL[slug]
    if slug in SKY:
        return SKY[slug]

    m = re.fullmatch(r"wind-beaufort-(\d+)", slug)
    if m:
        n = int(m.group(1))
        detail = BEAUFORT.get(n, "")
        return f"Wind force {n} on the Beaufort scale" + (f" ({detail})" if detail else "")

    m = re.fullmatch(r"wind-direction-([nsew]+)", slug)
    if m:
        d = WIND_DIR.get(m.group(1), m.group(1))
        return f"Wind from the {d}"

    for prefix in ("extreme-thunderstorms", "thunderstorms", "extreme", "overcast", "partly-cloudy", "mostly-clear"):
        if slug == prefix:
            base = SKY_TEMPLATES.get(prefix, prefix.replace("-", " "))
            return base
        if slug.startswith(prefix + "-"):
            rest = slug[len(prefix) + 1 :]
            base = SKY_TEMPLATES.get(prefix, prefix.replace("-", " "))
            return _describe_composite(base, rest)

    return slug.replace("-", " ").capitalize()


def main() -> None:
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    fill_slugs = sorted(p.name[:-4] for p in FILL.glob("*.svg"))

    lines: list[str] = [
        "# Meteocons weather icon reference (fill style)",
        "",
        "**Path:** `docs/plugins/weather-icons-reference.md`",
        "**Icons shown:** `plugins/weather/icons/fill/*.svg` (519 files)",
        "**Source manifest:** `plugins/weather/icons/manifest.json`",
        "",
        "Human-readable purpose for each Meteocons slug vendored for the Navi",
        "weather plugin. The same slugs exist in `flat/`, `line/`, and",
        "`monochrome/` with identical meaning — only the visual style differs.",
        "Animated counterparts (where present) use the same slug under",
        "`plugins/weather/animated-icons/fill/`.",
        "",
        "Plugin spec: [`weather-plugin.md`](weather-plugin.md). Regenerate this",
        "file after manifest updates:",
        "",
        "```bash",
        "python3 scripts/generate-weather-icons-reference.py",
        "```",
        "",
        "---",
        "",
        "## How to read a slug",
        "",
        "Most condition icons combine layers in the filename:",
        "",
        "```text",
        "{sky-cover}[-day|-night][-{precipitation-or-haze}]",
        "thunderstorms-{sky-cover}[-day|-night][-{precipitation}]",
        "extreme-thunderstorms-{sky-cover}[-day|-night][-{precipitation}]",
        "```",
        "",
        "| Token | Meaning |",
        "|---|---|",
        "| `mostly-clear` | Large clear area, some cloud |",
        "| `partly-cloudy` | Sun/moon and cloud share the sky |",
        "| `overcast` | Full cloud cover |",
        "| `extreme` | Very heavy precipitation or storm rate |",
        "| `thunderstorms` | Lightning present |",
        "| `extreme-thunderstorms` | Severe thunderstorm |",
        "| `-day` / `-night` | Sun or moon variant for map/HUD theme |",
        "| `-drizzle` | Light drizzle |",
        "| `-rain` | Rain |",
        "| `-sleet` | Rain and snow mixed |",
        "| `-snow` | Snow |",
        "| `-hail` | Hail |",
        "| `-fog` | Fog |",
        "| `-haze` | Haze / reduced visibility |",
        "| `-smoke` | Smoke or poor air quality |",
        "",
        "Example: `partly-cloudy-day-rain` = partly cloudy sky during the day with rain.",
        "",
        "---",
        "",
    ]

    for cat in manifest["categories"]:
        name = cat["name"]
        slug = cat["slug"]
        icons = [i["slug"] for i in cat["icons"] if i]
        lines.append(f"## {name}")
        lines.append("")
        if slug in ("mostly-clear", "partly-cloudy", "overcast", "extreme", "thunderstorms", "extreme-thunderstorms"):
            lines.append(
                f"Composite condition icons ({len(icons)} slugs). Each row is one "
                f"fill SVG at `plugins/weather/icons/fill/{{slug}}.svg`."
            )
            lines.append("")
        else:
            lines.append(
                f"{len(icons)} icons. File path: `plugins/weather/icons/fill/{{slug}}.svg`."
            )
            lines.append("")

        lines.append("| Slug | Purpose |")
        lines.append("|---|---|")
        for icon_slug in icons:
            desc = describe_slug(icon_slug)
            lines.append(f"| `{icon_slug}` | {desc} |")
        lines.append("")
        lines.append("---")
        lines.append("")

    # Orphans on disk but not in manifest (should not happen)
    manifest_slugs = {i["slug"] for c in manifest["categories"] for i in c["icons"] if i}
    orphans = [s for s in fill_slugs if s not in manifest_slugs]
    if orphans:
        lines.append("## Files not listed in manifest")
        lines.append("")
        for s in orphans:
            lines.append(f"- `{s}`")
        lines.append("")

    lines.append("## See also")
    lines.append("")
    lines.append("| Doc | Topic |")
    lines.append("|---|---|")
    lines.append("| [`weather-plugin.md`](weather-plugin.md) | Plugin spec and asset layout |")
    lines.append("| [`icons.md`](../icons.md#meteocons-weather-icons-mit--plugin-assets) | MIT licensing |")
    lines.append("| [`plugins.md`](../plugins.md) | Plugin host overview |")
    lines.append("")

    OUT.write_text("\n".join(lines), encoding="utf-8")
    print(f"Wrote {OUT} ({len(fill_slugs)} fill icons, {len(manifest['categories'])} categories)")


if __name__ == "__main__":
    main()
