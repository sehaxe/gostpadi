# gostpadi

Рисовалка блок-схем. Скармливаешь `main.c` — получаешь готовую схему:
все линии под 90° и одной толщины, стрелка — на входе в каждый блок,
`if` и `switch` расходятся ветками, `return` уводит стрелку в «End».
Ромбы с наклоном сторон 45°, циклы — шестиугольником «подготовка».
Рисовать руками не приходится.

![схема из кода C](docs/scheme-from-c.png)

Работает и в обратную сторону: короткий текстовый файл `.gvn` —
и та же схема без кода.

![switch](docs/scheme-switch.png)

Пакет на PyPI: [pypi.org/project/gostpadi](https://pypi.org/project/gostpadi/)
Онлайн-версия (схема из кода прямо в браузере):
[sehaxe.github.io/gostpadi](https://sehaxe.github.io/gostpadi/)

## Установка

```bash
pip install gostpadi
```

Нужны [matplotlib](https://matplotlib.org/) и [pycparser](https://github.com/eliben/pycparser) —
поставятся сами.

Вариант без установки — [uv](https://docs.astral.sh/uv/) запускает утилиту
прямо с GitHub:

```bash
uv run https://raw.githubusercontent.com/sehaxe/gostpadi/main/gostpadi.py main.c
```

## Использование

```bash
gostpadi main.c                 # -> main.png, вписано в А4
gostpadi main.c --show          # показать схему в терминале
gostpadi main.c -o scheme.svg   # векторный SVG для Word/LaTeX
gostpadi main.c --auto          # размер по схеме, без ужимания
gostpadi main.c --gvn           # сохранить текст схемы (можно править)
gostpadi 1/main.c 2/main.c 3/main.c 4/main.c -o scheme.png
```

Последний режим — **пачка схем**: каждой входной схеме рисуется свой
`scheme.png` рядом с ней (или в папке, если `-o папка/`), но размеры фигур
и масштаб страниц общие — во всей работе блоки всех схем одинаковые,
как и требуют правила оформления отчёта.

Флаг `--labels=ru` переключает все надписи схемы на русский
(«начало/конец», «да/нет»).

Понимает: `printf`/`scanf` (сами параллелограммы), присваивания, `if/else`
(в том числе `else if` и вложенные ветки), `switch/case/default` (кейсы висят
на линии из нижнего угла ромба, подписи `s = 1` … `default`), `while` и `for`
(шестиугольник «подготовка», тело возвращается в боковую вершину),
`return` (ветка уходит в «End»). Объявления переменных выбрасываются.
`do-while` пока нет — утилита скажет сама.

## Свой формат .gvn

Если хочется управлять каждой строкой — пиши схему текстом:
строка = блок, ветки — отступом в 4 пробела.

```text
#gostpadi 1
input scanf("%d", &x)
y = x * 2
if y > 10
    yes: printf("big")
    no:
    while y > 0
        y = y - 1
output printf("done")
```

Строка = блок, ветки и тело цикла — отступом в 4 пробела; внутри ветки можно
вкладывать свои `if`/`switch`/`while` (ещё +4 пробела). Строка без двоеточия
после метки ветки продолжает её содержимое.

Примеры и заготовка — в `examples/` и по команде `--template`.

## Все опции

| опция | что делает |
|---|---|
| `-o ФАЙЛ` | имя результата (`.png` или `.svg`); для пачки — имя рядом с каждым входом или папка |
| `--show` | нарисовать схему прямо в терминале |
| `--auto` | канвас по размеру схемы, без ужимания |
| `--gvn` | сохранить текст схемы |
| `--scale=N` | масштаб |
| `--font=N` | кегль (12) |
| `--lw=N` | толщина всех линий (1.0) |
| `--dpi=N` | плотность пикселей (200) |
| `--labels=ru\|en` | язык надписей: «Start/End» и «yes/no» (по умолчанию en) или «начало/конец» и «да/нет» |
| `--template` | заготовка схемы |

## Из Python

```python
import gostpadi

gostpadi.render(open("main.c").read(), "схема.png")          # из кода C
gostpadi.render(open("схема.gvn").read(), "результат.png")   # из .gvn
gostpadi.render(text, "x2.png", page="auto", scale=2.0)      # опции
```

## Проверка

```bash
uv run --with matplotlib --with pycparser python selftest.py
```

Прогоняет все вариации и сверяет геометрию с эталоном
(`tests/baseline.json`).

## Лицензия

MIT — [LICENSE](LICENSE).
