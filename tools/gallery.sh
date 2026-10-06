#!/usr/bin/env bash
# Галерея side-by-side: эталонный каталог против выхода движка.
# HTML без растеризации — <img> рендерит SVG, глаз сравнивает сам.
# usage: tools/gallery.sh <эталон/> <выход/> [out/]
set -euo pipefail
ref="${1:?эталон-каталог, напр. golden/}"
new="${2:?выход-каталог, напр. /tmp/opencode/dev-out/}"
out="${3:-/tmp/opencode/gostpadi-gallery}"
rm -rf "$out" && mkdir -p "$out/A" "$out/B"
cp "$ref"/*.svg "$out/A/"
cp "$new"/*.svg "$out/B/"

python3 - "$out" <<'PY'
import re, sys, html
from pathlib import Path

out = Path(sys.argv[1])
def key(name):  # страницы одной схемы в один ряд
    return re.sub(r"-\d+\.svg$", ".svg", name)

a, b = {p.name for p in (out/"A").glob("*.svg")}, {p.name for p in (out/"B").glob("*.svg")}
groups = {}
for n in a | b:
    groups.setdefault(key(n), set()).add(n)
rows = []
for g in sorted(groups):
    pa = " ".join(f'<img src="A/{html.escape(n)}">' for n in sorted(groups[g] & a))
    pb = " ".join(f'<img src="B/{html.escape(n)}">' for n in sorted(groups[g] & b))
    if not (groups[g] & a): pa = "<i>нет в эталоне</i>"
    if not (groups[g] & b): pb = "<i>нет в выходе</i>"
    rows.append(f"<h2>{html.escape(g[:-4])}</h2><div class=row><div>{pa}</div><div>{pb}</div></div>")
html_text = """<!doctype html><meta charset=utf-8>
<title>gostpadi: эталон | выход</title>
<style>
body{font:14px/1.4 system-ui;margin:16px;background:#fafafa}
h1{font-size:18px}h2{font-size:14px;margin:18px 0 4px}
.row{display:flex;gap:8px;align-items:flex-start}
.row>div{display:flex;gap:6px;flex:1;flex-wrap:wrap;background:#fff;border:1px solid #ddd;border-radius:8px;padding:6px}
img{max-width:100%;height:auto;border:1px solid #eee}
.legend{color:#555}
</style>
<h1>слева эталон (A), справа выход движка (B)</h1>
<p class=legend>страницы одной схемы — в одном ряду, по порядку</p>
""" + "\n".join(rows)
(out/"index.html").write_text(html_text)
PY
echo "галерея: $out/index.html"
