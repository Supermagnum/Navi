# Follow-up 51 head proof (`535ab13b`)

Debug APK on disk (not in git): `docs/fu51-map/head/app-debug.apk`.
sha256 `bdb350c5f3c68bb301f331055a13d43f5b661c1f587a1be2f1815e1345027b8d`.

Offline half: airplane, wifi/data off, `navi_fu49_force_offline`. Settled
measurement: two identical feature totals one second apart, 20 s cap.
Visual verdict is from the screenshots, not from count-agree flags.

## Matrix (zoom 7 to 15)

| Position | z | Off idle / back | On idle / back |
|---|---|---|---|
| Oslo | 7, 9, 11, 13, 15 | drawn / drawn | drawn / drawn |
| Hamar | 7, 9, 11, 13, 15 | drawn / drawn | drawn / drawn |
| Hallingdal (Bromma) | 7, 9, 11, 13, 15 | drawn / drawn | drawn / drawn |
| Östersund | 7, 9, 11, 13, 15 | drawn / drawn | drawn / drawn |
| Østlandet–Värmland | 7, 9, 11, 13, 15 | drawn / drawn | drawn / drawn |
| Hamburg | 7, 9, 11, 13, 15 | drawn / drawn | drawn / drawn |

Count-agree was false on Oslo z7 off, Hamar z7 off, Östersund z7 on, border
z7 on and Hamburg z7 on. Idle and back screenshots still match and are
drawn. Those are measurement races, not visual fails.

## One line per screenshot

### Start and extras

- `app_start.png`: planning sheet open; coast and sea already drawn behind it.
- `hamar_zoom_step_z3.png`: Europe overview, coasts and borders.
- `hamar_zoom_step_z5.png`: Norway drawn; Sweden is a flat mint fill. Fail.
- `hamar_zoom_step_z7.png`: Hamar and Mjøsa drawn.
- `hamar_zoom_step_z9.png`: Hamar drawn.
- `hamar_zoom_step_z11.png`: Hamar drawn.
- `hamar_zoom_step_z13.png`: Hamar drawn.
- `hamar_zoom_step_z15.png`: Hamar streets drawn.
- `border_pan_z11_0.png`: west of the border, lakes and roads.
- `border_pan_z11_1.png`: lakes and roads.
- `border_pan_z11_2.png`: Charlottenberg, both sides drawn.
- `border_pan_z11_3.png`: lakes and roads.
- `border_pan_z11_4.png`: east of the border, lakes and roads.
- `elsa_after_plan.png`: Elsa to Sjuvass on the overview; whole route visible.
- `elsa_route_fit.png`: same route framed without a map touch; land and coast drawn.
- `oslo_z11_rotated.png`: Oslo z11 landscape, fully drawn.

### Oslo

- `oslo_z7_off/idle.png` and `back.png`: Oslo and the fjord drawn (idle count was 0).
- `oslo_z7_on/idle.png` and `back.png`: drawn.
- `oslo_z9_off` and `_on`: drawn.
- `oslo_z11_off` and `_on`: city and harbour drawn.
- `oslo_z13_off` and `_on`: streets drawn.
- `oslo_z15_off` and `_on`: centre streets and labels drawn.

### Hamar

- `hamar_z7_off` idle/back: drawn (back count raced).
- `hamar_z7_on` through `hamar_z15_on`: Mjøsa, town and streets drawn.

### Hallingdal near Bromma

- `hallingdal_bromma_z7` through `z15`, off and on: Gol / Bromma, river and roads drawn.

### Östersund

- `ostersund_z7` through `z15`, off and on: Storsjön, town and streets drawn.

### Østlandet–Värmland border

- `ostlandet_varmland_z7` through `z15`, off and on: both sides of the border drawn.

### Hamburg

- `hamburg_z7` through `z15`, off and on: Elbe, city and streets drawn.

## Failures

1. `hamar_zoom_step_z5.png`: flat mint over Sweden. A regional archive's
   `earth` layer paints its header rectangle, including the neighbouring
   country. Same class as Oslo z5 / Hamburg z5.
2. Elsa overview z15 (64.889, 19.516) was not in this set. That point is
   forest with almost nothing to draw.

`taastrup_z12_on_recheck.png` is the later Taastrup online frame (387 / 66 / 43)
after the gate had recorded a blank count at 2.2 s. The picture was already
drawn; the gate now waits for a non-empty settled pair.

Zoom 7 to 15 at the six positions and the Elsa zoom-out are drawn. This
head APK is the tablet build, with the low-zoom limitation stated on
`docs/tablet-test.md`.
