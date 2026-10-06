#!/usr/bin/env python3
"""Структурный дифф SVG-каталогов: эталон golden/ против выхода движка.

Каждый файл сравнивается как отсортированный набор сигнатур элементов
(тег, атрибуты с округлёнными координатами, текст, дети). Порядок и
заголовки не важны, попиксельного сравнения нет — шрифты машинозависимы.

usage: svgdiff.py <эталон/> <выход/> [-v]
код возврата: 0 — всё совпало, 1 — есть расхождения.
"""
import re
import sys
from pathlib import Path

import xml.etree.ElementTree as ET


def num(v: str) -> str:
    try:
        f = float(v)
    except ValueError:
        return v
    return f"{f:.2f}".rstrip("0").rstrip(".")


def sig(el) -> tuple:
    attrs = tuple(sorted((k, num(v)) for k, v in el.attrib.items()))
    text = (el.text or "").strip()
    kids = tuple(sig(c) for c in el)
    return (el.tag, attrs, text, kids)


def load(path: Path) -> set:
    root = ET.parse(path).getroot()
    return {str(sig(c)) for c in root}


def parse_page(name: str) -> tuple[str, int]:
    """strany-2.svg и strany-01.svg → ('strany', 2): страница 1-based,
    старый движок первую не суффиксует, dev суффиксует -01."""
    m = re.match(r"^(.*?)(?:-(\d+))?\.svg$", name)
    return (m.group(1), int(m.group(2) or 1))


def main() -> int:
    args = [a for a in sys.argv[1:] if a != "-v"]
    verbose = "-v" in sys.argv
    ref, out = Path(args[0]), Path(args[1])
    pages: dict[str, dict[int, str]] = {}
    for dirname in (ref, out):
        d = {}
        for p in dirname.glob("*.svg"):
            stem, page = parse_page(p.name)
            d[(stem, page)] = p.name
        pages[dirname.name] = d
    pa, pb = pages[ref.name], pages[out.name]
    schemes = sorted({s for s, _ in pa | pb})
    bad = []
    for s in schemes:
        nums = sorted({n for (st, n) in pa | pb if st == s})
        for n in nums:
            ka, kb = (s, n) in pa, (s, n) in pb
            if ka and not kb:
                print(f"ONLY-эталон {s} стр.{n}")
                bad.append(s)
                continue
            if kb and not ka:
                print(f"ONLY-выход   {s} стр.{n}")
                bad.append(s)
                continue
            a, b = load(ref / pa[(s, n)]), load(out / pb[(s, n)])
            if a == b:
                print(f"MATCH {s} стр.{n}")
                continue
            print(f"DIFF {s} стр.{n}: в-эталоне {len(a - b)}, в-выходе {len(b - a)}")
            bad.append(s)
            if verbose:
                for x in sorted(a - b)[:6]:
                    print(f"  - {x[:110]}")
                for x in sorted(b - a)[:6]:
                    print(f"  + {x[:110]}")
    uniq = sorted(set(bad))
    print(f"=== схем: {len(schemes)}, чисто: {len(schemes) - len(uniq)},"
          f" расходится: {len(uniq)}")
    if uniq:
        for s in uniq:
            print(f"  расходится: {s}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
