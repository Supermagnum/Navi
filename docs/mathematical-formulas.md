# Mathematical formulas

Readable reference for how an implementation derives numbers (concrete code may
live in C or the host’s native language). Related protocol notes:
[`ECU.md`](ECU.md). Eco routing context: [`architecture.md`](../architecture.md).

## OBD-II — MAF-based fuel rate

When PID `0x5E` (engine fuel rate) is unavailable, estimate from mass air flow
(PID `0x10`):

$$
\text{Fuel rate (L/h)} = \frac{\text{MAF (g/s)} \times 3600}{\text{AFR} \times \rho \times 1000}
$$

- **MAF** — air mass flow (g/s)
- **AFR** — air–fuel ratio (e.g. ~14.7 for petrol; adjusted by fuel type /
  ethanol from PID `0x52`)
- **ρ** — fuel density (kg/L, e.g. ~0.745 for petrol)

## J1939 — fuel rate and fuel level

- **SPN 183** (PGN 65266 / FEEA): 0.05 L/h per bit, offset 0

$$
\text{fuel\_rate\_L\_h} = \text{raw\_spn183} \times 0.05
$$

- **SPN 96** (PGN 65276 / FEF4): 0.4 % per bit, range 0–250 %

$$
\text{fuel\_level\_%} = \text{raw\_spn96} \times 0.4
$$

$$
\text{fuel\_current\_L} = \frac{\text{fuel\_level\_%}}{100} \times \text{tank\_capacity\_L}
$$

## MegaSquirt — injector-based fuel rate

Inputs: injector pulse width `pw_ms` (ms), engine `rpm`, cylinder count
`n_cyl`, injector flow `flow_cc_min` (cc/min).

$$
\text{fuel\_rate\_L\_h} = \frac{\text{pw\_ms} \times \text{rpm} \times n_{\text{cyl}} \times \text{flow\_cc\_min}}{2{,}000{,}000}
$$

Rationale sketch: pulses/min scale with RPM; each pulse delivers
`flow_cc_min / 60` cc/s at 100 % duty; duty cycle is `pw_ms / 1000` of the
cycle. Out-of-range RPM / pulse width / rate updates are skipped.

## Range estimation

From current fuel and average consumption (`fuel_avg_consumption_x10` →
L/100 km):

$$
\text{range\_km} = \frac{\text{fuel\_current\_L} \times 100}{\text{consumption\_L\_per\_100km}}
$$

Short-term adaptive averages from live samples can refine this.

## Energy-based routing (eco-mode)

Segment energy cost is proportional to:

$$
E \approx (F_{\text{rolling}} + F_{\text{drag}}) \times d + m g \Delta h
$$

- \(F_{\text{rolling}}\) — rolling resistance
- \(F_{\text{drag}}\) — aerodynamic drag
- \(d\) — segment length
- \(m\) — total mass
- \(g\) — gravity
- \(\Delta h\) — height difference

Drag from user \(C_d\) and frontal area (\(A\)), with sea-level air density
\(\rho \approx 1.225\,\mathrm{kg/m^3}\):

$$
F_{\text{drag}} \propto \tfrac{1}{2}\,\rho\,C_d A\,v^2
$$

Temperature and elevation still adjust the coefficient inside each segment.
Rolling resistance uses a fixed coefficient times weight in the model
initializer. Live ECU fuel can be compared to energy predictions so
lower-energy (and thus lower-fuel) routes are preferred.
