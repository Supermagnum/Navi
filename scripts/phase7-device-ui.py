#!/usr/bin/env python3
"""Drive the Navi UI on the physical test device through adb only (Phase 7 acceptance).

  phase7-device-ui.py tap "Migrate now" [last]   tap the node whose text (or content-desc) contains the string
                                                 (with `last`: the last such node on screen)
  phase7-device-ui.py rowtap "Row text" "Button"  tap the button in the row of the text
  phase7-device-ui.py check "Text"               toggle the checkbox in the row of the text
  phase7-device-ui.py wait "Done and verified" 600   wait up to N seconds for a visible text
  phase7-device-ui.py texts                      print every visible text (one per line)
  phase7-device-ui.py shot name                  screenshot to test-artifacts/phase7/screens/<name>.png
  phase7-device-ui.py swipe up|down              scroll the focused list
  phase7-device-ui.py key BACK                   keyevent

Uses `uiautomator dump` and `input tap` on serial R52TB0JQEDE only (override with ANDROID_SERIAL, never an
emulator). No other app is touched.
"""
import os
import re
import subprocess
import sys
import time
import xml.etree.ElementTree as ET

SERIAL = os.environ.get("ANDROID_SERIAL", "R52TB0JQEDE")
if SERIAL.startswith("emulator-"):
    sys.exit("emulators are not allowed")
ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
SHOTS = os.path.join(ROOT, "test-artifacts", "phase7", "screens")
ADB = ["adb", "-s", SERIAL]


def adb(*args, check=True, binary=False):
    r = subprocess.run(ADB + list(args), capture_output=True, check=check)
    return r.stdout if binary else r.stdout.decode("utf-8", "replace")


def dump():
    for _ in range(4):
        out = adb("exec-out", "uiautomator", "dump", "/dev/tty", check=False)
        i = out.find("<?xml")
        j = out.rfind("</hierarchy>")
        if i >= 0 and j > i:
            return ET.fromstring(out[i : j + len("</hierarchy>")])
        time.sleep(0.7)
    raise SystemExit("uiautomator dump failed")


def nodes(root):
    for n in root.iter("node"):
        t = n.get("text") or ""
        d = n.get("content-desc") or ""
        m = re.match(r"\[(\d+),(\d+)\]\[(\d+),(\d+)\]", n.get("bounds") or "")
        if not m:
            continue
        x1, y1, x2, y2 = map(int, m.groups())
        yield {
            "text": t,
            "desc": d,
            "box": (x1, y1, x2, y2),
            "clickable": n.get("clickable") == "true",
            "checkable": n.get("checkable") == "true",
            "checked": n.get("checked") == "true",
        }


def find(root, needle, last=False):
    best = None
    for n in nodes(root):
        if needle.lower() in n["text"].lower() or needle.lower() in n["desc"].lower():
            if best is None or n["clickable"] or last:
                best = n
                if last and not n["clickable"]:
                    pass
    return best


def center(box):
    return (box[0] + box[2]) // 2, (box[1] + box[3]) // 2


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    cmd = sys.argv[1]
    if cmd == "tap":
        needle = sys.argv[2]
        last = len(sys.argv) > 3 and sys.argv[3] == "last"
        n = find(dump(), needle, last)
        if not n:
            sys.exit(f"not visible: {needle!r}")
        x, y = center(n["box"])
        adb("shell", "input", "tap", str(x), str(y))
        print(f"tapped {needle!r} at {x},{y}")
    elif cmd == "rowtap":
        # Tap the node with text B in the same row as the node with text A.
        a, b = sys.argv[2], sys.argv[3]
        root = dump()
        label = find(root, a)
        if not label:
            sys.exit(f"not visible: {a!r}")
        cy = (label["box"][1] + label["box"][3]) // 2
        cands = [n for n in nodes(root) if b.lower() in (n["text"] + n["desc"]).lower() and n["box"][1] <= cy <= n["box"][3]]
        if not cands:
            sys.exit(f"no {b!r} in the row of {a!r}")
        x, y = center(max(cands, key=lambda n: n["box"][0])["box"])
        adb("shell", "input", "tap", str(x), str(y))
        print(f"tapped {b!r} in the row of {a!r} at {x},{y}")
    elif cmd == "check":
        # Toggle the checkbox that sits in the same row as the given text.
        needle = sys.argv[2]
        root = dump()
        label = find(root, needle)
        if not label:
            sys.exit(f"not visible: {needle!r}")
        cy = (label["box"][1] + label["box"][3]) // 2
        boxes = [n for n in nodes(root) if n["checkable"] and n["box"][1] <= cy <= n["box"][3]]
        if not boxes:
            sys.exit(f"no checkbox in the row of {needle!r}")
        x, y = center(min(boxes, key=lambda n: n["box"][0])["box"])
        adb("shell", "input", "tap", str(x), str(y))
        print(f"toggled checkbox of {needle!r} at {x},{y}")
    elif cmd == "wait":
        needle, secs = sys.argv[2], float(sys.argv[3]) if len(sys.argv) > 3 else 30
        end = time.time() + secs
        while time.time() < end:
            if find(dump(), needle):
                print(f"visible: {needle!r}")
                return
            time.sleep(2)
        sys.exit(f"timeout waiting for {needle!r}")
    elif cmd == "texts":
        for n in nodes(dump()):
            if n["text"]:
                print(n["text"])
    elif cmd == "shot":
        os.makedirs(SHOTS, exist_ok=True)
        path = os.path.join(SHOTS, sys.argv[2] + ".png")
        with open(path, "wb") as fh:
            fh.write(adb("exec-out", "screencap", "-p", binary=True))
        print(path)
    elif cmd == "swipe":
        w, h = 1200, 2000
        up = sys.argv[2] == "up"
        adb("shell", "input", "swipe", "600", "1500" if up else "500", "600", "500" if up else "1500", "400")
    elif cmd == "key":
        adb("shell", "input", "keyevent", sys.argv[2])
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
