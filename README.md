# gostpadi

Рисовалка блок-схем. Скармливаешь `main.c` — получаешь готовую схему:
все линии под 90° и одной толщины, `if` и `switch` расходятся ветками,
`return` уводит стрелку в «End». Рисовать руками не приходится.

![схема из кода C](docs/scheme-from-c.png)

Работает и в обратную сторону: короткий текстовый файл `.gvn` —
и та же схема без кода.

![switch](docs/scheme-switch.png)

## Установка

Достаточно [uv](https://docs.astral.sh/uv/), ничего ставить не нужно:

```bash
curl -LsSf https://astral.sh/uv/install.sh | sh     # Linux / macOS
```

Схема из кода одной командой, без клонирования:

```bash
uv run https://raw.githubusercontent.com/sehaxe/gostpadi/main/gostpadi.py main.c
```

Классический вариант:

```bash
git clone https://github.com/sehaxe/gostpadi && cd gostpadi
pip install matplotlib
python gostpadi.py main.c
```

## Как пользоваться

```bash
gostpadi main.c                 # -> main.png, вписано в А4
gostpadi main.c --show          # показать схему в терминале
gostpadi main.c -o scheme.svg   # векторный SVG для Word/LaTeX
gostpadi main.c --auto          # размер по схеме, без ужимания
gostpadi main.c --gvn           # сохранить текст схемы (можно править)
```

Понимает: `printf`/`scanf` (сами параллелограммы), присваивания, `if/else`,
`switch/case/default` (кейсы висят на линии из нижнего угла ромба),
`return` (ветка уходит в «End»). Объявления переменных выбрасываются.
Циклов и `else if` пока нет — утилита скажет сама.

Хочешь управлять каждой строкой — пиши `.gvn` руками: строка = блок,
ветки — отступ в 4 пробела. Примеры в `examples/`.

## Опции

| опция | что делает |
|---|---|
| `-o ФАЙЛ` | имя результата (`.png` или `.svg`) |
| `--show` | нарисовать схему прямо в терминале |
| `--auto` | канвас по размеру схемы, без ужимания |
| `--gvn` | сохранить текст схемы |
| `--scale=N` | масштаб |
| `--font=N` | кегль (12) |
| `--lw=N` | толщина всех линий (1.4) |
| `--dpi=N` | плотность пикселей (200) |
| `--template` | заготовка схемы |

## Проверка

```bash
uv run --with matplotlib python selftest.py
```

Прогоняет все вариации и сверяет геометрию с эталоном
(`tests/baseline.json`).

## Лицензия

MIT — [LICENSE](LICENSE).
