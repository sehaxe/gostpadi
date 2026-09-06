#!/usr/bin/env python3
"""Самопроверка gostpadi: рендер всех вариаций + инварианты раскладки.

Запуск:  uv run --with matplotlib --with pycparser python selftest.py
Печатает OK/FAIL по каждой проверке; код возврата 0, если всё прошло.
"""
import glob
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import gostpadi as gp  # noqa: E402
from PIL import Image  # noqa: E402

FAILS = []
SCRIPT = os.path.join(HERE, "gostpadi.py")


def check(name, cond, detail=""):
    tag = "OK  " if cond else "FAIL"
    print(f"{tag}  {name}" + (f"  [{detail}]" if detail and not cond else ""))
    if not cond:
        FAILS.append(name)


def on_border(pt, sh, eps=1.6):
    """Точка лежит на границе фигуры (прямоугольник/ромб/круг)."""
    x, y = pt
    cx, cy, w, h, k = sh["cx"], sh["cy"], sh["w"], sh["h"], sh["kind"]
    if k == "conn":
        return abs(((x - cx) ** 2 + (y - cy) ** 2) ** 0.5 - w / 2) <= eps
    if k == "if":
        return abs(abs(x - cx) / (w / 2) + abs(y - cy) / (h / 2) - 1) <= 0.09
    return (abs(abs(x - cx) - w / 2) <= eps) or (abs(abs(y - cy) - h / 2) <= eps)


def check_layout(name, text):
    """Разбить на части, отложить и проверить инварианты каждой части."""
    nodes = gp.parse(text)
    sizes = gp.normalize(nodes)
    parts = gp.split_scheme(nodes, sizes)
    good = True
    for pi, part in enumerate(parts):
        tag = f"{name} / лист {pi + 1}"
        shapes, edges, labels, bounds, _a = gp.layout(part, sizes)
        minx, miny, W, H = bounds

        # 1. один тип фигур — один размер
        seen = {}
        for sh in shapes:
            key, size = sh["kind"], (round(sh["w"], 6), round(sh["h"], 6))
            if key in seen and seen[key] != size:
                check(f"{tag}: одинаковые размеры «{key}»", False,
                      f"{seen[key]} vs {size}")
                good = False
            seen[key] = size

        # 2. фигуры не накладываются
        for a in range(len(shapes)):
            for b in range(a + 1, len(shapes)):
                A, B = shapes[a], shapes[b]
                ox = min(A["cx"] + A["w"] / 2, B["cx"] + B["w"] / 2) - \
                     max(A["cx"] - A["w"] / 2, B["cx"] - B["w"] / 2)
                oy = min(A["cy"] + A["h"] / 2, B["cy"] + B["h"] / 2) - \
                     max(A["cy"] - A["h"] / 2, B["cy"] - B["h"] / 2)
                if ox > 1.0 and oy > 1.0:
                    check(f"{tag}: без наложений фигур", False,
                          f"{A['kind']}×{B['kind']} перекрытие {ox:.0f}×{oy:.0f}")
                    good = False

        # 3. линии ортогональны; начало на границе фигуры; стрелка входит в фигуру
        for e in edges:
            pts = e["points"]
            for (x1, y1), (x2, y2) in zip(pts, pts[1:]):
                if abs(x1 - x2) > 0.01 and abs(y1 - y2) > 0.01:
                    check(f"{tag}: линии под 90°", False,
                          f"{(x1, y1)} -> {(x2, y2)}")
                    good = False
            # начало линии: на границе фигуры ИЛИ на другой линии
            # (спуски кейсов начинаются прямо на гребёнке)
            def near_line(p, other):
                for (ax, ay), (bx, by) in zip(other, other[1:]):
                    dx, dy = bx - ax, by - ay
                    l2 = dx * dx + dy * dy
                    if l2 == 0:
                        continue
                    t = max(0.0, min(1.0, ((p[0] - ax) * dx +
                                           (p[1] - ay) * dy) / l2))
                    if (abs(p[0] - (ax + t * dx)) <= 0.75 and
                            abs(p[1] - (ay + t * dy)) <= 0.75):
                        return True
                return False

            starts_ok = any(on_border(pts[0], sh) for sh in shapes)
            if not starts_ok:
                for e2 in edges:
                    if e2 is e:
                        continue
                    if near_line(pts[0], e2["points"]):
                        starts_ok = True
                        break
            if not starts_ok:
                check(f"{tag}: линия выходит из фигуры", False, f"{pts[0]}")
                good = False
            if e.get("arrow", True):
                ok = any(on_border(pts[-1], sh) for sh in shapes)
                # стрелка может кончаться и в точке слияния (ГОСТ):
                # либо помеченной точкой, либо на другой линии потока
                if not ok and e.get("dots"):
                    ok = True
                if not ok:
                    for e2 in edges:
                        if e2 is e:
                            continue
                        if near_line(pts[-1], e2["points"]):
                            ok = True
                            break
                if not ok:
                    check(f"{tag}: стрелка входит в фигуру", False,
                          f"{pts[-1]}")
                    good = False

        # 4. подписи в границах листа
        for l in labels:
            if not (minx <= l["x"] <= minx + W and miny <= l["y"] <= miny + H):
                check(f"{tag}: подпись в границах листа", False, l["text"])
                good = False

    check(f"{name}: рендер", True)
    return good


CASES = {
    "линейная": "a = 1\nb = 2\nc = a + b",
    "ввод-вывод авто": 'scanf("%d", &x)\nprintf("%d", x)',
    "if/else обе ветки": ('if a > 0\n    да: printf("плюс")\n'
                          '    нет: printf("минус")'),
    "if одна ветка": 'if a > 0\n    да: printf("плюс")',
    "if -> конец + нет + поток": ('input scanf("%d", &a)\n'
                                  "if scanf != 1\n"
                                  '    да -> end: printf("err"); return 1\n'
                                  "    нет:\n"
                                  "b = a * 2\n"
                                  'output printf("%d", b)'),
    "if -> конец без нет": ("input scanf(\"%d\", &a)\n"
                            "if scanf != 1\n"
                            "    да -> end: printf(\"err\"); return 1\n"
                            "b = 1"),
    "switch 2": 'if switch (x)\n    1: printf("один")\n    2: printf("два")',
    "switch 3": ("if switch (a)\n"
                         "    1: printf(y); break\n"
                         "    2: printf(z); break\n"
                         "    иначе: a = 10; break"),
    "switch 4": ("if switch (s)\n"
                 '    1: printf("1")\n    2: printf("2")\n'
                 '    3: printf("3")\n    4: printf("4")'),
    "switch 5": ("if switch (s)\n"
                 '    1: printf("1")\n    2: printf("2")\n'
                 '    3: printf("3")\n    4: printf("4")\n'
                 '    иначе: printf("5")'),
    "switch 6": ("if switch (s)\n"
                 '    1: printf("1")\n    2: printf("2")\n'
                 '    3: printf("3")\n    4: printf("4")\n'
                 '    5: printf("5")\n    6: printf("6")'),
    "switch 7": ("if switch (s)\n"
                 '    1: printf("1")\n    2: printf("2")\n'
                 '    3: printf("3")\n    4: printf("4")\n'
                 '    5: printf("5")\n    6: printf("6")\n'
                 '    иначе: printf("7")'),
    "switch -> конец в кейсе": ("if switch (s)\n"
                                '    1 -> end: printf("раз"); return 1\n'
                                '    2: printf("два")\n'
                                '    иначе: printf("три")'),
    "английские ключевые слова": ('input scanf("%d", &a)\n'
                                 "if a > 0\n"
                                 '    yes: printf("plus")\n'
                                 "output printf(a)"),
    "перенос длинных строк": ("b = a / 100000 + a / 10000 % 10 + "
                              "a / 1000 % 10 + a % 10"),
    "комментарии": '// привет\ninput scanf("%d", &a) # хвост\n action b = 1',
    "разбивка на листы": "\n".join(f"x{i} = x{i} + {i}" for i in range(14)),
    "if -> конец и switch (task4)": ('input scanf("%d", &s)\n'
                                     "if s < 1 || s > 4\n"
                                     '    да -> end: printf("нет"); return 1\n'
                                     "    нет:\n"
                                     "switch (s)\n"
                                     '    1: printf("зима"); break\n'
                                     '    2: printf("весна"); break\n'
                                     '    3: printf("лето"); break\n'
                                     '    4: printf("осень"); break\n'
                                     '    иначе: printf("?")'),
}

C_SRCS = {
    "examples/main.c": os.path.join(HERE, "examples", "main.c"),
    "switch с return в кейсе": '''#include <stdio.h>
int main(void) {
    int a;
    scanf("%d", &a);
    switch (a) {
        case 1:
            printf("раз");
            return 2;
        case 2:
            printf("два");
            break;
        default:
            a = 9;
    }
    printf("%d", a);
    return 0;
}''',
    "объявления и инициализация": '''#include <stdio.h>
int main(void) {
    float a, b;
    int n = 5;
    double x = 1.5;
    a = n * 2;
    printf("%f", x);
    return 0;
}''',
    "if без else": '''#include <stdio.h>
int main(void) {
    int a;
    scanf("%d", &a);
    if (a > 0)
        printf("плюс");
    printf("готово");
    return 0;
}''',
}


def check_c(name, src):
    try:
        gvn = gp.c_to_gvn(src)
    except gp.ParseError as e:
        check(f"C: {name}", False, str(e))
        return
    check_layout(f"C: {name}", gvn)


def expected_error(name, args, want_code):
    r = subprocess.run([sys.executable, SCRIPT] + args, capture_output=True,
                       text=True)
    check(f"CLI: код возврата {want_code} ({name})", r.returncode == want_code,
          f"получен {r.returncode}")


def main():
    print("== .gvn: вариации ==")
    for name, text in CASES.items():
        try:
            check_layout(name, text)
        except Exception as e:  # noqa: BLE001
            check(name, False, f"{type(e).__name__}: {e}")

    print("== код C ==")
    for name, src_or_path in C_SRCS.items():
        src = src_or_path
        if src_or_path.endswith(".c"):
            src = open(src_or_path, encoding="utf-8").read()
        try:
            check_c(name, src)
        except Exception as e:  # noqa: BLE001
            check(f"C: {name}", False, f"{type(e).__name__}: {e}")

    print("== файлы examples/ ==")
    for path in sorted(glob.glob(os.path.join(HERE, "examples", "*.gvn"))):
        name = os.path.basename(path)
        try:
            check_layout(name, open(path, encoding="utf-8").read())
        except Exception as e:  # noqa: BLE001
            check(f"examples/{name}", False, f"{type(e).__name__}: {e}")

    print("== эталон геометрии ==")
    base_path = os.path.join(HERE, "tests", "baseline.json")
    if os.path.exists(base_path):
        import json
        import snapshot as sn
        base = json.load(open(base_path, encoding="utf-8"))
        for name, text in CASES.items():
            cur = json.dumps(sn.snapshot_layout(name, text),
                             ensure_ascii=False, sort_keys=True)
            check(f"эталон: {name}",
                  cur == json.dumps(base.get(name), ensure_ascii=False,
                                    sort_keys=True),
                  "геометрия изменилась — обновите snapshot.py")
    else:
        print("  baseline.json нет — запустите snapshot.py")

    print("== CLI ==")
    with tempfile.TemporaryDirectory() as td:
        gvn = os.path.join(td, "ok.gvn")
        open(gvn, "w", encoding="utf-8").write(
            'input scanf("%d", &a)\noutput printf(a)\n')
        out = os.path.join(td, "ok.png")
        r = subprocess.run([sys.executable, SCRIPT, gvn, "-o", out],
                           capture_output=True, text=True)
        check("CLI: рендер с -o", r.returncode == 0 and os.path.exists(out),
              r.stderr[-120:] if r.returncode else "")
        out_svg = os.path.join(td, "ok.svg")
        r = subprocess.run([sys.executable, SCRIPT, gvn, "-o", out_svg],
                           capture_output=True, text=True)
        svg_ok = r.returncode == 0 and os.path.exists(out_svg) and \
                 "<svg" in open(out_svg, encoding="utf-8").read(300)
        check("CLI: рендер в SVG", svg_ok, "")
        r = subprocess.run([sys.executable, SCRIPT, gvn, "--show"],
                           capture_output=True, text=True,
                           env={**os.environ, "GOSTPADI_SHOW": "kitty"})
        check("CLI: --show (kitty-протокол)",
              r.returncode == 0 and "\x1b_G" in r.stdout, "")
        r = subprocess.run([sys.executable, SCRIPT, gvn, "--show"],
                           capture_output=True, text=True,
                           env={**os.environ, "GOSTPADI_SHOW": "none"})
        check("CLI: --show без графики", r.returncode == 0, "")

    # русские ключевые слова больше не часть языка — отказ с кодом 1
    with tempfile.TemporaryDirectory() as td2:
        ru = os.path.join(td2, "ru.gvn")
        open(ru, "w", encoding="utf-8").write(
            'если a > 0\n    да: printf("плюс")\n')
        expected_error("CLI: русские ключевые слова отклоняются", [ru], 1)

    expected_error("CLI: нет файла", ["нет.gvn"], 1)
    expected_error("CLI: неизвестный флаг", ["--что", "x.gvn"], 2)
    expected_error("CLI: без аргументов", [], 2)

    print()
    if FAILS:
        print("ПРОВАЛЕНО:", "; ".join(FAILS))
        return 1
    print("ВСЕ ПРОВЕРКИ ПРОЙДЕНЫ")
    return 0


if __name__ == "__main__":
    sys.exit(main())
