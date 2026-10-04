#!/usr/bin/env python3
"""Обрезает SVG листа по содержимому.

Лист A4 — это 595x842 pt, а схема занимает на нём меньше половины.
Для превью на сайте пустое поле вокруг схемы делает блоки нечитаемыми,
поэтому viewBox сжимается до габарита нарисованного плюс небольшой
отступ. Полотно остаётся белым — так выглядит как лист.

Использование: crop.py <вход.svg> <выход.svg> [отступ_pt]
"""
import re
import sys


def bounds(svg: str, pw: float, ph: float, tx: float, ty: float, sc: float):
    """Габарит всего, что нарисовано, в координатах пользователя."""
    xs, ys = [], []

    # <rect x= y= width= height=>
    for m in re.finditer(
        r'<rect[^>]*?\sx="(-?[\d.]+)"[^>]*?\sy="(-?[\d.]+)"'
        r'[^>]*?\swidth="([\d.]+)"[^>]*?\sheight="([\d.]+)"', svg
    ):
        x, y, w, h = map(float, m.groups())
        # подложка во весь лист — это фон страницы, а не содержимое
        if w >= pw - 1 and h >= ph - 1:
            continue
        x, y, w, h = x * sc + tx, y * sc + ty, w * sc, h * sc
        xs += [x, x + w]
        ys += [y, y + h]

    # <polygon points="x,y x,y …">
    for m in re.finditer(r'<polygon[^>]*?\spoints="([^"]+)"', svg):
        for pair in m.group(1).split():
            if "," in pair:
                px, py = pair.split(",", 1)
                xs.append(float(px) * sc + tx)
                ys.append(float(py) * sc + ty)

    # <polyline points="…"> — рёбра
    for m in re.finditer(r'<polyline[^>]*?\spoints="([^"]+)"', svg):
        for pair in m.group(1).split():
            if "," in pair:
                px, py = pair.split(",", 1)
                xs.append(float(px) * sc + tx)
                ys.append(float(py) * sc + ty)

    # <text …> — подпись занимает не только свою точку: при
    # text-anchor=middle она расходится на половину ширины в обе
    # стороны, при end — влево. Без этого габарит уже схемы и по
    # краям блоки срезаны. Ширина моноширинного глифа ≈ 0.61 кегля
    # (см. char_w в style.rs).
    for m in re.finditer(r'<text\b([^>]*)>(.*?)</text>', svg, re.S):
        attrs, body = m.group(1), m.group(2)

        def attr(name, default=None):
            a = re.search(rf'\b{name}="([^"]*)"', attrs)
            return a.group(1) if a else default

        try:
            x, y = float(attr("x")), float(attr("y"))
        except (TypeError, ValueError):
            continue
        anchor = attr("text-anchor", "start")
        size = float(attr("font-size", 14))
        chars = len("".join(re.findall(r'<tspan[^>]*>([^<]*)</tspan>', body)) or body.strip())
        w = chars * size * 0.61 * sc
        x, y = x * sc + tx, y * sc + ty
        if anchor == "middle":
            xs += [x - w / 2, x + w / 2]
        elif anchor == "end":
            xs += [x - w, x]
        else:
            xs += [x, x + w]
        ys.append(y)
    return (min(xs), min(ys), max(xs), max(ys)) if xs and ys else None


def transform(svg: str):
    """Перенос и масштаб корневой группы <g>.

    Координаты фигур и подписей — локальные для этой группы, а не
    страничные. Без её применения габарит уезжает в минус (схема
    строится вокруг оси x = 0) и обрезка берёт только правую половину.
    """
    m = re.search(
        r'<g[^>]*?\stransform="translate\((-?[\d.]+)[ ,]+(-?[\d.]+)\)\s*'
        r'(?:scale\(([\d.]+)\))?',
        svg,
    )
    if not m:
        return 0.0, 0.0, 1.0
    tx, ty, s = m.group(1), m.group(2), m.group(3)
    return float(tx), float(ty), float(s) if s else 1.0


def page_size(svg: str):
    """Размер листа из атрибутов корневого <svg>."""
    m = re.search(r'<svg[^>]*?width="([\d.]+)"[^>]*?height="([\d.]+)"', svg)
    return (float(m.group(1)), float(m.group(2))) if m else (595.276, 841.89)


def main() -> int:
    src, dst = sys.argv[1], sys.argv[2]
    pad = float(sys.argv[3]) if len(sys.argv) > 3 else 10.0
    svg = open(src, encoding="utf-8").read()

    pw, ph = page_size(svg)
    tx, ty, sc = transform(svg)
    box = bounds(svg, pw, ph, tx, ty, sc)
    if box is None:
        sys.exit("в svg нечего обрезать")

    x0, y0, x1, y1 = box
    # Подписи с text-anchor выходят за свою точку, и габарит может
    # вылезти за лист. Обрезаем по видимой части страницы.
    pw, ph = page_size(svg)
    x0, y0 = max(0.0, x0 - pad), max(0.0, y0 - pad)
    x1, y1 = min(pw, x1 + pad), min(ph, y1 + pad)
    w, h = x1 - x0, y1 - y0
    if w <= 0 or h <= 0:
        sys.exit("содержимое вне листа")

    # подложка: белый прямоугольник по обрезанному viewBox, иначе фон
    # страницы уезжает за пределы обрезки
    head, rest = svg.split("?>", 1)
    rest = rest.strip()
    inner = rest[rest.index(">") + 1 : rest.rindex("</svg>")]
    out = (
        f'{head}?>\n'
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{w:.2f}" height="{h:.2f}" '
        f'viewBox="{x0:.2f} {y0:.2f} {w:.2f} {h:.2f}">'
        f'<rect x="{x0:.2f}" y="{y0:.2f}" width="{w:.2f}" height="{h:.2f}" fill="#fff"/>'
        f"{inner}</svg>\n"
    )
    open(dst, "w", encoding="utf-8").write(out)
    print(f"{dst}: {w:.0f}x{h:.0f} pt (было 595x842)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
