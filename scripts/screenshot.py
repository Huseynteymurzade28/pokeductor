#!/usr/bin/env python3
"""Takes a README screenshot of the interface without a terminal to shoot.

Runs the release binary in a detached 130x40 tmux session, sends it keys,
captures the pane with its colours, lays the cells out on a fixed grid in an
HTML page and has headless Chromium photograph it at 1204x772, the size of
every `assets/ui-*.png`.

    cargo build --release
    scripts/screenshot.py assets/ui-help.png - -- '?'
    scripts/screenshot.py assets/ui-party.png party.json -- p@3
    scripts/screenshot.py out.png - gengar -- c text=/ text=garchomp Enter@4 h@8

The second argument is a session file to start from (a party, a collection),
or `-` for a fresh one; the app runs against a state directory under
`target/shots`, never the real one. Arguments up to `--` go to pokeductor.
After it, each word is a tmux key name, or `text=...` to type literally, with
an optional `@seconds` to wait afterwards for a fetch to land.

Needs tmux, chromium and the JetBrains Mono font. Standard library only.
"""
import html
import os
import re
import subprocess
import sys
import time
import unicodedata

out, session = sys.argv[1], sys.argv[2]
rest = sys.argv[3:]
split = rest.index("--")
args, keys = rest[:split], rest[split + 1:]
root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
here = os.path.join(root, "target", "shots")
state = os.path.join(here, "state")
os.makedirs(os.path.join(state, "pokeductor"), exist_ok=True)
path = os.path.join(state, "pokeductor", "session.json")
if session == "-":
    if os.path.exists(path):
        os.remove(path)
else:
    open(path, "w").write(open(session).read())

binary = os.path.join(root, "target", "release", "pokeductor")
cmd = f"env XDG_STATE_HOME={state} COLORTERM=truecolor TERM=xterm-256color {binary} --no-mouse " + " ".join(args)
tmux = ["tmux", "-L", "pdshot", "-f", "/dev/null"]
subprocess.run(tmux + ["kill-server"], stderr=subprocess.DEVNULL)
subprocess.run(tmux + ["new-session", "-d", "-s", "pd", "-x", "130", "-y", "40", cmd], check=True)
time.sleep(4)
for key in keys:
    key, _, wait = key.partition("@")
    if key.startswith("text="):
        subprocess.run(tmux + ["send-keys", "-t", "pd", "-l", key[5:]], check=True)
    else:
        subprocess.run(tmux + ["send-keys", "-t", "pd", key], check=True)
    time.sleep(float(wait or 0.6))
ansi = subprocess.run(tmux + ["capture-pane", "-t", "pd", "-e", "-p"], check=True, capture_output=True).stdout.decode()
subprocess.run(tmux + ["kill-server"])

BASE, TEXT = "#1d2b53", "#fff1e8"
BASIC = ["#000000", "#cd0000", "#00cd00", "#cdcd00", "#0000ee", "#cd00cd", "#00cdcd", "#e5e5e5",
         "#7f7f7f", "#ff0000", "#00ff00", "#ffff00", "#5c5cff", "#ff00ff", "#00ffff", "#ffffff"]
def c256(n):
    if n < 16: return BASIC[n]
    if n >= 232:
        v = 8 + (n - 232) * 10; return f"#{v:02x}{v:02x}{v:02x}"
    n -= 16; f = lambda v: 0 if v == 0 else 55 + v * 40
    return f"#{f(n // 36):02x}{f(n // 6 % 6):02x}{f(n % 6):02x}"

st = dict(fg=None, bg=None, bold=False, rev=False, dim=False, ital=False, und=False)
def sgr(params):
    p = [int(x) if x else 0 for x in params.split(";")] if params else [0]
    i = 0
    while i < len(p):
        n = p[i]
        if n == 0: st.update(fg=None, bg=None, bold=False, rev=False, dim=False, ital=False, und=False)
        elif n == 1: st["bold"] = True
        elif n == 2: st["dim"] = True
        elif n == 3: st["ital"] = True
        elif n == 4: st["und"] = True
        elif n == 7: st["rev"] = True
        elif n == 22: st["bold"] = st["dim"] = False
        elif n == 23: st["ital"] = False
        elif n == 24: st["und"] = False
        elif n == 27: st["rev"] = False
        elif 30 <= n <= 37: st["fg"] = BASIC[n - 30]
        elif 90 <= n <= 97: st["fg"] = BASIC[n - 82]
        elif 40 <= n <= 47: st["bg"] = BASIC[n - 40]
        elif 100 <= n <= 107: st["bg"] = BASIC[n - 92]
        elif n == 39: st["fg"] = None
        elif n == 49: st["bg"] = None
        elif n in (38, 48):
            key = "fg" if n == 38 else "bg"
            if p[i + 1] == 2:
                st[key] = "#%02x%02x%02x" % tuple(p[i + 2:i + 5]); i += 4
            else:
                st[key] = c256(p[i + 2]); i += 2
        i += 1

body = []
for line in ansi.rstrip("\n").split("\n"):
    row, pos = [], 0
    st.update(fg=None, bg=None, bold=False, rev=False, dim=False, ital=False, und=False)
    for m in re.finditer(r"\x1b\[([0-9;]*)m|\x1b\[[0-9;?]*[A-Za-z]|\x1b[()][A-Z0-9]|[\x0e\x0f]", line):
        text = line[pos:m.start()]
        if text: row.append((dict(st), text))
        if m.group(1) is not None: sgr(m.group(1))
        pos = m.end()
    if line[pos:]: row.append((dict(st), line[pos:]))
    spans = []
    for s, text in row:
        fg, bg = s["fg"] or TEXT, s["bg"] or BASE
        if s["rev"]: fg, bg = bg, fg
        css = f"color:{fg};background:{bg};"
        if s["bold"]: css += "font-weight:bold;"
        if s["ital"]: css += "font-style:italic;"
        for ch in text:
            wide = unicodedata.east_asian_width(ch) in ("W", "F")
            w = 18 if wide else 9
            spans.append(f'<span style="{css}width:{w}px;">{html.escape(ch)}</span>')
    body.append("".join(spans))
page = f"""<!doctype html><meta charset="utf-8"><style>
html,body{{margin:0;background:{BASE};}}
pre{{margin:0;padding:14px 16px;background:{BASE};color:{TEXT};font-family:'JetBrainsMono Nerd Font Mono','JetBrains Mono',monospace;font-size:15px;line-height:1.24;}}
span{{display:inline-block;height:1.24em;vertical-align:top;overflow:hidden;white-space:pre;}}
</style><pre>{chr(10).join(body)}</pre>"""
page_path = os.path.join(here, "page.html")
open(page_path, "w").write(page)
subprocess.run(["chromium", "--headless", "--disable-gpu", "--hide-scrollbars", "--window-size=1204,772",
                f"--screenshot={os.path.abspath(out)}", page_path], check=True, stderr=subprocess.DEVNULL, stdout=subprocess.DEVNULL)
print(out, len(ansi))
