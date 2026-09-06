# gostpadi

Рисовалка блок-схем. Скармливаешь `main.c` — получаешь готовую схему:
все линии под 90° и одной толщины, `if` и `switch` расходятся ветками,
`return` уводит стрелку в «End». Рисовать руками не приходится.

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
```

Флаг `--labels=ru` переключает все надписи схемы на русский
(«начало/конец», «да/нет»).

Понимает: `printf`/`scanf` (сами параллелограммы), присваивания, `if/else`,
`switch/case/default` (кейсы висят на линии из нижнего угла ромба,
подписи `s = 1` … `default`), `return` (ветка уходит в «End»).
Объявления переменных выбрасываются. Циклов и `else if` пока нет —
утилита скажет сама.

## Свой формат .gvn

Если хочется управлять каждой строкой — пиши схему текстом:
строка = блок, ветки — отступом в 4 пробела.

```text
#gostpadi 1
input scanf("%d", &x)
y = x * 2
if y > 10
    yes: printf("big")
    no: printf("small")
output printf("done")
```

Примеры и заготовка — в `examples/` и по команде `--template`.

## Все опции

| опция | что делает |
|---|---|
| `-o ФАЙЛ` | имя результата (`.png` или `.svg`) |
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
