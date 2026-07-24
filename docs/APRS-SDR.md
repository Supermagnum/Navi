# APRS SDR DSP and RTL-SDR

Reference for a future APRS receive path that tunes an RTL-SDR dongle, demodulates
Bell 202 AFSK (AX.25 UI frames), and delivers decoded information fields into
Navi’s [`TrackStore`](../core/src/tracks/mod.rs).

**Status in this repository:** packet ingest / DSP is **not implemented** yet.
Moving-icon display and range filtering already exist (see [`APRS.md`](APRS.md)).
This document captures the expected demodulation contract (bit-stream stages,
hardware constraints) so a plugin or native host crate can match it.

## Planned hardware crate: `rtl-sdr-rs`

| Item | Value |
|---|---|
| Crate | [`rtl-sdr-rs`](https://crates.io/crates/rtl-sdr-rs) |
| Docs | <https://docs.rs/rtl-sdr-rs> |
| Upstream | <https://github.com/ccostes/rtl-sdr-rs> |
| Role | Open RTL-SDR USB devices, set centre frequency / sample rate / gain, read IQ |

Navi should treat `rtl-sdr-rs` as the **preferred Rust front-end** for IQ capture
on hosts that expose USB (desktop, and Android via USB host / accessory when
available). The trusted routing core still must not own the USB device; a T0/T1
plugin or host process owns the dongle and pushes decoded stations into
`TrackStore` / UniFFI.

Example open (illustrative — not wired in-tree):

```rust
use rtl_sdr_rs::RtlSdr;

let mut radio = RtlSdr::open_with_index(0).expect("open RTL-SDR");
// set sample rate (e.g. 192_000), tune with IF offset (see below), then read IQ
```

Optional Cargo feature `rtl_sdr_blog` enables RTL-SDR Blog hardware tweaks
(documented upstream).

---

## Expected decode pipeline (bit-stream stages)

Target payload expectation (what the DSP must ultimately deliver to the
callback after FCS removal):

```text
frame_callback: length=40 first4=82 A0 A4 A6
```

### Important

The demodulation pipeline makes **bit decisions from DC-centered FM
discriminator output** using a **bit-timing PLL** (phase accumulator, optional
zero-crossing nudge), **per-symbol averaging**, and **threshold 0.0**, not
Goertzel.

### Conventions

| Topic | Rule |
|---|---|
| Bit order | AX.25 bytes are transmitted **LSB-first** |
| Mark / space | mark = 1200 Hz, space = 2200 Hz |
| NRZI line bit | 1 = mark, 0 = space (tone selection) |
| NRZI data rule | transition ⇒ data 0, no transition ⇒ data 1 |
| Samples / bit (RF) | 192000 / 1200 = **160** RF samples/bit |
| Samples / bit (audio) | after decimate-by-4: 48000 / 1200 = **40** audio samples/bit |

Bit examples below focus on the start of the stream (preamble flags and the
first payload byte) — the region most sensitive to off-by-one errors.

### Stage 1/12 — AX.25 bytes (pre-stuff, pre-NRZI)

First payload bytes after the opening flag begin with the destination address:

```text
payload bytes start: 82 A0 A4 A6 ...
```

### Stage 2/12 — AX.25 data bits (LSB-first)

AX.25 flag `0x7E` and first payload byte `0x82`:

```text
0x7E bits: 0 1 1 1 1 1 1 0
0x82 bits: 0 1 0 0 0 0 0 1

data bits:  0 1 1 1 1 1 1 0  0 1 0 0 0 0 0 1
           ^ flag (0x7E)      ^ first payload byte (0x82)
```

### Stage 3/12 — Bit stuffing (TX) / de-stuffing (RX)

- Flags are **never** stuffed.
- Stuffing applies only to frame content **between** flags.

**Boundary condition (critical):** the de-stuffer must not discard bits while
assembling a flag candidate byte (the 8 bits used to decide whether a completed
byte is `0x7E`). Discarding a stuffed 0 during flag assembly slips the decoder
by one bit; the classic symptom is first payload byte `0x45` instead of `0x82`.

RX rule:

- De-stuff only the **payload bit stream between flags**.
- Do not run the de-stuffer on bits that belong to a flag byte (opening,
  preamble flush flags, or closing flag).

```text
before stuffing: ... 1 1 1 1 1 1 ...
after stuffing:  ... 1 1 1 1 1 0 1 ...
                                ^ stuffed 0
```

On RX, discard the 0 after five consecutive 1s (payload only).

### Stage 4/12 — NRZI line bits (TX output)

For flag `0 1 1 1 1 1 1 0` and initial NRZI state = 1 (idle mark):

```text
data bits:  0 1 1 1 1 1 1 0
line bits:  0 0 0 0 0 0 0 1
```

(data 0 toggles line state; data 1 keeps it.)

### Stage 5/12 — Tone stream from NRZI line bits

- line 1 ⇒ mark (1200 Hz)
- line 0 ⇒ space (2200 Hz)

```text
line bits:  0 0 0 0 0 0 0 1
tones:      S S S S S S S M
```

### Stage 6/12 — FM discriminator (audio-rate)

Discriminator output is a scalar proportional to instantaneous frequency.
Sanity-check lines (values vary slightly per run):

```text
verify[0]: ... audio=0.000000 freq=0.0Hz
verify[1]: ... audio=0.289064 freq=2208.3Hz
```

Approximate (uncentered): space ≈ 0.29, mark ≈ 0.17.

`verify[0]` only seeds the discriminator’s previous-sample state; the first
meaningful estimate is `verify[1]`.

### Stage 7/12 — Per-bit averaging (nominal 40 audio samples / PLL symbol)

Accumulate discriminator samples until the PLL phase wraps. A slow IIR removes
DC from the `atan2` discriminator so mark and space straddle zero; averages are
then negative for mark and positive for space.

Uncentered example (before DC removal in logs):

```text
avg_disc[0..7] (example): 0.29 0.29 0.29 0.29 0.29 0.29 0.29 0.17
                           S    S    S    S    S    S    S    M
```

### Stage 8/12 — Raw bit decision (threshold 0.0 on DC-centered avg)

```text
raw_bit = 1 (mark)  if avg < 0.0
raw_bit = 0 (space) if avg >= 0.0
```

The older fixed midpoint **0.23** applied to **uncentered** `atan2` (mark ~0.17,
space ~0.29). DC tracking replaces that with a zero threshold.

On real hardware, separation and DC behaviour are signal-dependent; tune
`fm_dc_alpha` and optional PLL gain `pll_alpha` as needed.

Using the uncentered example averages (conceptually mirrored after DC removal):

```text
avg_disc: 0.29 0.29 0.29 0.29 0.29 0.29 0.29 0.17
raw_bit:  0    0    0    0    0    0    0    1
```

### Stage 9/12 — NRZI decode (RX)

- no transition (same `raw_bit` as `last_bit`) ⇒ decoded 1
- transition ⇒ decoded 0

With `last_bit` init = 1 and the example flag raw bits:

```text
last_bit init: 1
raw_bit:       0 0 0 0 0 0 0 1
decoded:       0 1 1 1 1 1 1 0
```

→ flag bits for `0x7E`.

### Stage 10/12 — Flag search

Flag pattern (LSB-first): `0 1 1 1 1 1 1 0`.

Expected lock diagnostics:

```text
FLAG_FOUND at decoded=8
FLAG_FOUND at decoded=8 goertzel_blk=8
FLAG_FOUND last_bit=1
```

Log fields may still use historical `goertzel_*` names; counters correspond to
**bit periods** (one per 40 audio samples), not Goertzel blocks on the main path.

### Stage 11/12 — In-frame bits after flag lock

Next 8 decoded bits must be `0x82` LSB-first. First four payload bytes
`82 A0 A4 A6` as a bit stream:

```text
0x82: 0 1 0 0 0 0 0 1
0xA0: 0 0 0 0 0 1 0 1
0xA4: 0 0 1 0 0 1 0 1
0xA6: 0 1 1 0 0 1 0 1

0 1 0 0 0 0 0 1   0 0 0 0 0 1 0 1   0 0 1 0 0 1 0 1   0 1 1 0 0 1 0 1
```

### Stage 12/12 — Frame callback

```text
frame_callback: length=40 first4=82 A0 A4 A6
```

### Final DSP stats (synthetic integration expectation)

```text
APRS SDR DSP stats: rf_samples=76960 audio_samples=19240 decim_factor=4
  samples_per_bit=40 goertzel_block=40 goertzel_blocks=481
  raw_bits=481 decoded_bits=481 flags_found=2 frames=1
```

`flags_found` counts opening flag (search lock) and closing flag (end of frame).

### Implementation footguns

**No static locals for HDLC/AX.25 state.** Keep in the DSP instance struct:

- `flag_pos`
- `in_frame_bit_pos`
- `in_frame_current_byte`
- `bit_stuff_count`

**Preamble flush and the ≥ 15 guard.** When a completed byte equals `0x7E`
while already `in_frame`:

- If `frame_buffer_pos < 15`: preamble flag — reset byte assembly + stuffing,
  keep `in_frame = 1`.
- If `frame_buffer_pos >= 15`: closing flag — deliver frame (without 2-byte
  FCS), then reset and exit frame.

### Quick fault reference

| Symptom | Likely cause |
|---|---|
| `flags_found=0` | Raw-bit polarity wrong, or DC centering / threshold-at-zero wrong |
| `flags_found=2 frames=0` | Closing-flag handling broken, or stuck in preamble flush |
| Many `frame_byte[*]=0x7E` then no payload | Preamble flush should keep `in_frame=1`; or unstable mark/space |
| Truncated `length` | Closing flag too early, or unstable timing / spurious `0x7E` |
| Never closes / huge `length` | Closing flag never recognized, or unstable bits so `0x7E` never forms |
| First payload `0x45` (want `0x82`) | De-stuffer ran during flag assembly |
| Payload `0xBE` | Byte/bit position not reset on preamble flush, or leaked static state |
| `DISCARD stuffed zero` during flags | De-stuffer active while assembling a flag |
| Wrong `first4` | PLL alignment, DC centering, or destuff/NRZI boundary violated |
| `first4=82 A0 A4 A6` | Correct |

---

## SDR basics and RTL-SDR hardware artifacts

### What an SDR produces (IQ)

Interleaved byte stream:

```text
I0 Q0 I1 Q1 I2 Q2 ...
```

Conceptually \( x[n] = I[n] + j Q[n] \). Tuned to centre \( f_c \), offsets from
\( f_c \) appear as positive/negative baseband frequencies.

### Digital downconversion (mixer / IF offset)

Tune hardware slightly away from the APRS channel (IF offset), then mix IQ by
\( e^{-j 2\pi f_{\mathrm{mix}} n / f_s} \) so the signal sits where the FM
discriminator expects it.

Phase increment sketch: \( \Delta\phi = -2\pi \cdot f_{\mathrm{offset}} / f_s \).

### Decimation (RF → audio)

Typical test rates:

| Rate | Value |
|---|---|
| RF sample rate | 192000 Hz |
| Decimation | 4 |
| Audio sample rate | 48000 Hz |

### FM discriminator

Common form: \( y[n] = \arg(x[n] \cdot \overline{x[n-1]}) \).

Before DC tracking, short-term averages cluster roughly at mark ~0.17 and space
~0.29 in the synthetic case; subtract tracked DC so decisions use threshold
**0.0** (see stages 7–8).

### Bit decisions (PLL + average + threshold)

Not Goertzel on the main path:

1. Track/subtract DC (`fm_dc` / `fm_dc_alpha`)
2. Bit-timing PLL (`1 / samples_per_bit` per audio sample; optional
   `pll_alpha` zero-crossing nudge — default **0** for stable synthetic counts)
3. Average DC-centered samples until PLL wrap (nominal **40** samples/bit)
4. Compare average to **0.0** (mark if negative, space if non-negative)

### Why DC-centered reception fails on real RTL-SDR

**DC spike:** LO leakage puts a strong artifact at 0 Hz baseband. If the APRS
signal is tuned to DC, the spike lands on the signal and often ruins decode.

**IQ imbalance:** gain/phase mismatch creates a spectral mirror near DC and
hurts SNR.

**Synthetic tests at 0 Hz IF** still pass because generated IQ has no DC spike,
no imbalance, and no front-end noise — they prove DSP math, not hardware
suitability.

When `if_offset_hz = 0`, the mixer is a no-op (\( \cos 0 + j\sin 0 = 1 \)).

### Practical guidance for real hardware

- Prefer a **non-zero IF offset** (typically **50–100 kHz**, e.g. +100 kHz).
- Tune the RTL-SDR centre above the APRS channel by that offset so the DC spike
  sits at \( -f_{\mathrm{offset}} \), then digitally downmix back for the
  discriminator.
- Capture IQ with **`rtl-sdr-rs`**; do not rely on DC-centered reception for
  field use.

---

## Mapping into Navi

```text
rtl-sdr-rs (IQ) → mixer/decimate → FM disc + PLL bits → AX.25/HDLC
  → APRS information field parse → TrackStation upsert → map overlay
```

| Stage | Navi home (today / planned) |
|---|---|
| IQ capture | Planned: host/`rtl-sdr-rs` (not in core) |
| DSP / AX.25 | Planned: APRS SDR plugin or native host module |
| Station store + range | Implemented: `driver_break_core::tracks` |
| Map markers | Implemented: Compose overlay / MapLibre hooks |

See also: [`APRS.md`](APRS.md) (protocol fields + range filtering),
[`PROTOCOLS.md`](PROTOCOLS.md), [`architecture.md`](architecture.md) (T0
sensor tier).
