#!/usr/bin/env python3
"""Host-side virtual-sensor injector for ImuCalibrationCampaignInstrumentedTest.

Listens for logcat lines:
  ImuCampaign: REQUEST:<name>
and replies with:
  ImuCampaign: ACK:<name>
after applying `adb emu sensor set` for ANDROID_SERIAL (must be emulator-5554).
"""
from __future__ import annotations

import os
import subprocess
import sys
import time

SERIAL = os.environ.get("ANDROID_SERIAL", "emulator-5554")
ADB = os.environ.get("ADB", "adb")
G = 9.80665

MOUNT_15_10 = (1.7029069015174023, -2.4995876205578753, 9.328587997892189)
VEHICLE_8_3 = (1.1834990735332243, -1.1763951458786903, 9.663633303587167)
FLAT = (0.0, 0.0, G)


def adb(*args: str, check: bool = True) -> str:
    cmd = [ADB, "-s", SERIAL, *args]
    r = subprocess.run(cmd, capture_output=True, text=True, check=False)
    if check and r.returncode != 0:
        raise RuntimeError(f"adb failed ({r.returncode}): {' '.join(cmd)}\n{r.stderr}")
    return (r.stdout or "") + (r.stderr or "")


def emu_sensor(xyz: tuple[float, float, float]) -> None:
    payload = f"{xyz[0]}:{xyz[1]}:{xyz[2]}"
    adb("emu", "sensor", "set", "acceleration", payload)
    adb("emu", "sensor", "set", "gravity", payload, check=False)


def emu_gyro(xyz: tuple[float, float, float]) -> None:
    adb("emu", "sensor", "set", "gyroscope", f"{xyz[0]}:{xyz[1]}:{xyz[2]}")


def still() -> None:
    emu_gyro((0.0, 0.0, 0.0))
    adb("emu", "sensor", "set", "linear-acceleration", "0:0:0", check=False)


def apply(name: str) -> None:
    if name == "ping":
        return
    if name == "tilt_mount_15_10":
        still()
        emu_sensor(MOUNT_15_10)
        return
    if name == "vehicle_tilt_8_3":
        still()
        emu_sensor(VEHICLE_8_3)
        return
    if name == "flat":
        still()
        emu_sensor(FLAT)
        return
    if name == "motion_gyro":
        emu_sensor(MOUNT_15_10)
        emu_gyro((0.0, 0.0, 0.55))
        adb("emu", "sensor", "set", "linear-acceleration", "1.4:0.2:-0.3", check=False)
        return
    raise ValueError(f"unknown request: {name}")


def main() -> int:
    if SERIAL != "emulator-5554":
        print(f"refusing non-emulator serial: {SERIAL}", file=sys.stderr)
        return 2
    adb("logcat", "-c", check=False)
    print(f"watching logcat ImuCampaign on {SERIAL}", flush=True)
    proc = subprocess.Popen(
        [ADB, "-s", SERIAL, "logcat", "-v", "brief", "-s", "ImuCampaign:I"],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
    )
    assert proc.stdout is not None
    last_motion = 0.0
    try:
        while True:
            line = proc.stdout.readline()
            if not line:
                if proc.poll() is not None:
                    print("logcat exited", file=sys.stderr)
                    return 1
                time.sleep(0.05)
                continue
            line = line.strip()
            if "REQUEST:" not in line:
                continue
            name = line.split("REQUEST:", 1)[1].strip().split()[0]
            print(f"request: {name}", flush=True)
            try:
                apply(name)
                # Broadcast ACK via logcat so the test can poll.
                adb(
                    "shell",
                    "log",
                    "-t",
                    "ImuCampaign",
                    f"ACK:{name}",
                )
                print(f"acked: {name}", flush=True)
                if name == "motion_gyro":
                    last_motion = time.time()
            except Exception as e:
                print(f"FAILED {name}: {e}", file=sys.stderr)
                adb("shell", "log", "-t", "ImuCampaign", f"ACK:ERROR:{name}", check=False)
            # Refresh motion injection while recently requested.
            if time.time() - last_motion < 20:
                try:
                    apply("motion_gyro")
                except Exception:
                    pass
    finally:
        proc.kill()


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        raise SystemExit(0)
