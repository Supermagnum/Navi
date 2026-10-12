# VR-N76 Step 5 — read-only reachability

Date: 2026-10-12
Host adapter: 3C:DC:03:69:8C:D1 (haaken-pc)
Radio MAC: 38:D2:00:01:4B:E7 (paired, trusted)
Operator: VFO B selected; never-transmit; audio RFCOMM UUID not opened.

## Result: PASS

| Step | Result | Notes |
|---|---|---|
| Connect | ok | RFCOMM channel 1 (also answered on ch 4 earlier) |
| GET_DEV_INFO | ok ~17 ms | vendor=0x01 product=0x0103 (259); gate allowlist extended |
| REGISTER_NOTIFICATION | ok | no reply expected (HTCommander) |
| GET_HT_STATUS | ok ~24 ms | tx=false sq=false rx=false doubleChannel=2 (VFO B) |
| READ_SETTINGS | ok ~13 ms | channel_a=0 channel_b=6 double_channel=2 |
| READ_RF_CH A | ok ~12 ms | RX 434.6500 / TX 432.6500 MHz |
| READ_RF_CH B | ok ~14 ms | RX 145.5125 / TX 145.5125 MHz |
| FREQ_MODE_GET_STATUS | ok ~11 ms | raw saved (Q3) |

VFO B fingerprint: `testdata/cat/hardware/vr-n76/VFO_B_fingerprint.hex`

## Protocol notes confirmed on hardware

- GET_DEV_INFO request payload must be `[3]` (empty payload yields a short/wrong reply).
- productId on this unit is `0x0103`, not classic `0x0002`.
- `support_vfo` bit clear on this firmware → scratch-channel path is primary.
- With VFO B selected (doubleChannel=2), Step 6 uses scratch channel A (slot 0).
