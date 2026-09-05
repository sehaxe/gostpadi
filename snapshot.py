#!/usr/bin/env python3
"""Снимок геометрии всех тестовых схем — эталон для регрессионных тестов.

Запуск:  uv run --with matplotlib python snapshot.py
Пишет tests/baseline.json (координаты фигур, линий и подписей).
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import gostpadi as gp  # noqa: E402

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
    "switch 3 (доска)": ("if switch (a)\n"
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


def r3(v):
    return round(v, 3)


def snapshot_layout(name, text):
    nodes = gp.parse(text)
    sizes = gp.normalize(nodes)
    parts = gp.split_scheme(nodes, sizes)
    out = []
    for part in parts:
        shapes, edges, labels, bounds, _a = gp.layout(part, sizes)
        out.append({
            "shapes": sorted(
                (sh["kind"], r3(sh["cx"]), r3(sh["cy"]),
                 r3(sh["w"]), r3(sh["h"]), tuple(sh["lines"]))
                for sh in shapes),
            "edges": sorted(
                (tuple((r3(x), r3(y)) for x, y in e["points"]),
                 bool(e.get("arrow", True)))
                for e in edges),
            "labels": sorted(
                (r3(l["x"]), r3(l["y"]), l["text"], l["ha"])
                for l in labels),
            "bounds": tuple(r3(v) for v in bounds),
        })
    return out


def main():
    base = {}
    for name, text in CASES.items():
        base[name] = snapshot_layout(name, text)
    for name, path in C_SRCS.items():
        src = open(path, encoding="utf-8").read() if path.endswith(".c") else path
        base["C: " + name] = snapshot_layout(name, gp.c_to_gvn(src))
    for path in sorted(glob_examples()):
        name = "file: " + os.path.basename(path)
        base[name] = snapshot_layout(name, open(path, encoding="utf-8").read())

    dst = os.path.join(HERE, "tests", "baseline.json")
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    with open(dst, "w", encoding="utf-8") as f:
        json.dump(base, f, ensure_ascii=False, indent=1, sort_keys=True)
    print(f"эталон: {dst} ({len(base)} схем)")


def glob_examples():
    import glob
    return glob.glob(os.path.join(HERE, "examples", "*.gvn"))


if __name__ == "__main__":
    main()
