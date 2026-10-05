#!/usr/bin/env python3
"""Подставляет настоящие схемы движка в главную страницу.

Примеры на главной не нарисованы руками: берём вывод
`gostpadi --labels=ru` из examples/ и вставляем на место. Иначе
картинки на странице и на сайте приложения разъезжаются, а проверить
их нечем — правка страницы ломала бы вид, но не содержание.

Использование: examples.py [путь-к-gostpadi]
"""
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
INDEX = ROOT / "docs" / "index.html"
OUT = pathlib.Path("/tmp/gostpadi-examples")

# Подпись карточки: ключ в index.html -> (файл примера, заголовок, текст)
CARDS = [
    # герой: та же схема, что и в последней карточке, — он и был её
    # источником, пока карточки не стали отдельными слотами
    ("fig-hero", "simple.c", "", ""),
    ("fig-nested", "nested_loops.c", "Цикл в цикле",
     "for внутри while — оба возвращают поток в начало"),
    ("fig-switch", "switch_case.c", "Диспетчер",
     "switch: кейсы стоят одной шиной в один ряд, break — отдельным блоком"),
    ("fig-io", "msvc_scanf_s.c", "Разбор и проверка ввода",
     "scanf_s, три условия подряд, четыре выхода"),
    ("fig-simple", "simple.c", "Простое ветвление",
     "scanf, проверка, два printf"),
]


def run(binary: pathlib.Path, src: pathlib.Path, dest: pathlib.Path) -> pathlib.Path:
    """Один пример -> один SVG с русскими надписями, ровно по содержимому.

    Через `--trim`, а не через лист A4 с последующей обрезкой: trim уже
    отдаёт viewBox по габариту схемы и без `<g transform>`, а обрезка
    листа A4 этот transform разворачивает — и портит viewBox, отрезав
    схему по половине. Три схемы на главной выглядели порубленными
    именно из-за этого.
    """
    dest.mkdir(parents=True, exist_ok=True)
    out = dest / (src.stem + ".svg")
    subprocess.run(
        [str(binary), "--labels=ru", "--trim", "-o", str(out), str(src)],
        check=True,
        capture_output=True,
    )
    return out


def main() -> int:
    binary = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "target" / "release" / "gostpadi"
    if not binary.exists():
        print(f"нет бинаря {binary}, собери cargo build --release", file=sys.stderr)
        return 1
    html = INDEX.read_text(encoding="utf-8")
    changed = 0

    # Герой и карточки помечены комментариями с ключом.
    for key, name, title, caption in CARDS:
        svg = run(binary, ROOT / "examples" / name, OUT).read_text(encoding="utf-8").strip()
        html, n = re.subn(
            rf'(<div class="(?:fig|sheet)" data-key="{key}">).*?(</div>)',
            lambda m: m.group(1) + svg + m.group(2),
            html,
            count=1,
            flags=re.S,
        )
        if n != 1:
            print(f"не найден слот {key}", file=sys.stderr)
            return 1
        changed += n
        if title:
            html = re.sub(
                rf'(<figcaption data-key="{key}"><b>).*?(</b>).*?(</figcaption>)',
                lambda m: m.group(1) + title + m.group(2) + caption + m.group(3),
                html,
                flags=re.S,
            )

    INDEX.write_text(html, encoding="utf-8")
    print(f"подставлено схем: {changed}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())