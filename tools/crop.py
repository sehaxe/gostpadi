#!/usr/bin/env python3
"""Переписывает viewBox каждого SVG на content-box (атрибут data-box
создаёт движок dev) с запасом по вкусу. Полный лист A4 с полями мешает
смотреться и рендерится в волосной штрих — так сравниваем содержимое.

usage: crop.py <вход-директоред/> <выход/> [запас_pt=15]
"""
import re
import sys
from pathlib import Path

src, dst = Path(sys.argv[1]), Path(sys.argv[2])
pad = float(sys.argv[3]) if len(sys.argv) > 3 else 15.0
dst.mkdir(parents=True, exist_ok=True)
n = 0
for p in sorted(src.glob("*.svg")):
    t = p.read_text()
    m = re.search(r'data-box="([\d.\- ]+)"', t)
    if not m:
        continue  # нет content-box — это уже плотный SVG (эталон)
    x, y, w, h = (float(v) for v in m.group(1).split())
    vb = f"{x - pad:.3f} {y - pad:.3f} {w + 2 * pad:.3f} {h + 2 * pad:.3f}"
    t = re.sub(r'viewBox="[^"]*"', f'viewBox="{vb}"', t, count=1)
    (dst / p.name).write_text(t)
    n += 1
print(f"обрезано: {n}")
