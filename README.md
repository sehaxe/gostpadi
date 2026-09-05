# gostpadi

**Блок-схемы по ГОСТ 19.701-90 прямо из кода на C** — или из своего
текстового формата `.gvn`, если хочется управлять каждой строкой.

Скармливаешь `main.c` — получаешь готовую схему: все линии строго под 90°
и одной толщины, фигуры одного типа — одного размера, `switch` веером
кейсов из нижнего угла ромба, `return` уводит ветку в «конец». Ничего
рисовать руками не надо.

![схема из кода C](docs/scheme-from-c.png)

![switch](docs/scheme-switch.png)

## Установка

Ничего ставить не нужно, если есть [uv](https://docs.astral.sh/uv/) —
утилита сама скачает скрипт и matplotlib:

```bash
# установка uv (Linux / macOS)
curl -LsSf https://astral.sh/uv/install.sh | sh

# установка uv (Windows PowerShell)
powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"
```

Всё, можно пользоваться (пример — на C-файле из этого репозитория):

```bash
uv run https://raw.githubusercontent.com/sehaxe/gostpadi/main/gostpadi.py \
    examples/main.c --show
```

Классический вариант — клонировать репозиторий:

```bash
git clone https://github.com/sehaxe/gostpadi
cd gostpadi
uv run gostpadi.py examples/main.c --show
# или без uv:
pip install matplotlib
python gostpadi.py examples/main.c --show
```

## Главное: схема из кода C

```bash
gostpadi main.c                    # -> main.png (вписано в А4)
gostpadi main.c --show             # + показать в терминале
gostpadi main.c --labels=ru        # подписи «да/нет» (по умолчанию ru)
gostpadi main.c --gvn              # сохранить ещё и текст схемы main.gvn
```

Понимает: `printf`/`scanf` (сами параллелограммы), присваивания, `if/else`
(ветки плитками, `return` уводит ветку в «конец»), `switch/case/default`
(ромб, из нижнего угла горизонтальная линия — на ней все кейсы,
подписи `month = 1` … `default`). Объявления переменных выбрасываются,
длинные `printf` сокращаются до `printf("Начало фразы...")`.

Не понимает (и честно скажет): циклы `while/for/do`, `else if`, вложенные
`if` внутри веток, `goto`.

## Схема из текста .gvn (необязательно)

Если хочется управлять каждой строкой — есть свой текстовый формат:
строка = блок, порядок строк = порядок блоков сверху вниз, ветки — отступом
в 4 пробела. `Start`/`End` добавляются сами. Ключевые слова только
английские — никакого двуязычия.

```text
#gostpadi 1
input scanf("%d", &x)          # parallelogram (input)
y = x * 2                      # rectangle (action); keyword optional
if y > 10                      # diamond; branches indented 4 spaces
    yes: printf("big")
    no: printf("small")
output printf("done")          # printf/scanf are detected automatically
```

Всего пять слов: `input`, `output`, `action`, `if`, `yes`/`no`;
у switch метки кейсов — `1:`, `2:` … и `default:` (подписи станут
`s = 1`, `default`), а `yes -> end:` уводит ветку прямо в «End».
Длинные строки переносятся внутри фигур сами, комментарии — `#` и `//`.

## Все опции CLI

| опция | что делает |
|---|---|
| `-o ФАЙЛ` | имя PNG (по умолчанию `<имя входа>.png`) |
| `--auto` | канвас по размеру схемы, но не больше А4; без разбиения на листы |
| `--scale=N` | принудительный масштаб |
| `--font=N` | базовый кегль (по умолчанию 12) |
| `--lw=N` | толщина **всех** линий и рамок (по умолчанию 1.4) |
| `--dpi=N` | плотность пикселей (по умолчанию 200) |
| `--show` | нарисовать результат прямо в терминале (kitty/WezTerm/Ghostty/iTerm2) |
| `--gvn` | при входе `.c` сохранить ещё и текст схемы `.gvn` |
| `--template` | напечатать заготовку `.gvn` |

## Из Python

```python
import gostpadi

gostpadi.render(open("main.c").read(), "схема.png")            # из кода C
gostpadi.render(open("схема.gvn").read(), "результат.png")     # из .gvn
gostpadi.render(text, "x2.png", page="auto", scale=2.0)        # опции
```

Если схема не влезает в А4, `render` сам делит её на листы
(`результат.png`, `результат-2.png`, …), соединяя кружками «А», «Б»…

## Проверка

```bash
uv run --with matplotlib python selftest.py
```

Прогоняет все вариации (if/else, switch на 2–7 кейсов, `-> конец`,
разбивка на листы, оба языка, код C, коды ошибок CLI) и проверяет
инварианты ГОСТ на каждой сгенерированной схеме.

## Соответствие ГОСТ 19.701-90

- **Символы** — по стандарту: терминатор, процесс, данные (параллелограмм),
  решение (ромб), соединитель (кружок). Ромб с несколькими выходами
  (switch) стандартом прямо разрешён.
- **Линии** — ортогональные, одной толщины, сливаются в точках, входят
  в символы со стрелками; текст внутри символов; однотипные символы —
  одного размера; выходы решения — из боковых вершин.
- **Осознанные упрощения** (обычны для учебных схем): все `return` ведут в
  общий «конец»; `break` нарисован как процесс; стрелки стоят на всех входах.

## Примеры

В `examples/`: `main.c` — программа, из которой строится схема;
`hello.gvn` — минимум на английском; `board1_linear.gvn`, `board2_if.gvn`,
`board3_switch.gvn` — шаблоны «как на доске»; `task1..4.gvn` — схемы
реальных лабораторных задач (см. картинки выше).

## Лицензия

MIT — см. [LICENSE](LICENSE).
'''
print("readme ok")
PYEOF
git add -A && git commit -q -m "README: установка uv, главный сценарий — схема из кода C; новые превью" && git push -q origin main && echo PUSHED && git log --oneline | head -2
__zcode_status=$?
if [ "$__zcode_status" -eq 0 ]; then pwd -P > '/tmp/zcode-4d730357-779d-4a9f-9033-bb8272fc50bc-cwd'; fi
exit "$__zcode_status"