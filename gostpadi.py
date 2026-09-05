#!/usr/bin/env python3
"""gostpadi — блок-схемы по ГОСТ из своего текстового формата .gvn.

Идея: вы описываете только текст блоков и порядок — раскладку библиотека
делает сама, строго по шаблону:

    - основной поток — вертикальная линия по центру;
    - «да» ветвления уходит налево, «нет»/очередной case — направо,
      средняя ветка switch — прямо вниз (как на доске);
    - инструкции ветки через «;» превращаются в отдельные плитки
      колонки (printf(a) -> break);
    - все линии строго под 90° и одной толщины, стрелки входят в фигуры;
    - все фигуры одного типа в схеме имеют одинаковый размер;
    - если схема не влезает в лист А4, она сама режется на части:
      часть кончается кружком «А», следующая начинается кружком «А».

Формат .gvn (построчный, как markdown; «начало» и «конец» добавляются сами;
первой строкой можно написать «#gostpadi 1» — это просто подпись формата):

    # комментарий (и // тоже)
    ввод scanf("%d", &a)              # параллелограмм (ввод)
    b = a / 100000 + ...              # прямоугольник (действие)
    вывод printf("Гипотенуза...")     # параллелограмм (вывод)
    если a < 100000 || a > 999999     # ромб; ветки ниже с отступом (4 пробела)
        да -> конец: printf("Ош..."); return 1   # ветка уходит в конец
        нет:                          # пустая ветка = подпись на линии
    если switch (status)              # веток может быть сколько угодно
        1: printf("Декабрь...")       # подпись станет «status = 1»
        иначе: printf("Как ты сюда попал...")    # подпись станет «default»

Строка без ключевого слова тоже действие; printf/scanf и т.п. сами
становятся вводом-выводом. Условие ромба автоматически оформляется
как «if (...)» (кроме уже начинающихся с if/switch).

Использование:
    gostpadi схема.gvn                     # -> схема.png
    gostpadi a.gvn b.gvn c.gvn             # пачка файлов за раз
    gostpadi схема.gvn -o отчёт/рис.png    # своё имя PNG
    gostpadi схема.gvn --show              # показать прямо в терминале
    gostpadi схема.gvn --auto --scale=2    # канвас по контенту, крупнее
    gostpadi --template > новая.gvn        # заготовка схемы
Опции: --auto, --scale=N, --font=N, --lw=N (толщина всех линий),
--dpi=N, --show (kitty/WezTerm/Ghostty/iTerm2), --template, -o, --version.

или из python:
    import gostpadi
    gostpadi.render(open("схема.gvn").read(), "результат.png")

Зависимости: matplotlib (ставятся сами при запуске через uv).
"""

# /// script
# requires-python = ">=3.9"
# dependencies = ["matplotlib"]
# ///
from dataclasses import dataclass
import base64
import os
import re
import shutil
import sys

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Circle, FancyArrowPatch, FancyBboxPatch, Polygon, Rectangle

# ---------- настройки оформления ----------

__version__ = "1.1"


@dataclass(frozen=True)
class Style:
    """Все параметры оформления схем в одном месте.

    Меняйте поля — геометрия подстроится сама; жёстко зашитых чисел
    в раскладке нет.
    """

    # шрифт и текст
    font: float = 12.0            # базовый кегль, пункты
    font_stack: tuple = ("DejaVu Sans Mono", "Noto Sans CJK TC", "DejaVu Sans")
    char_w: float = 7.3           # ширина символа моношрифта (0.61em)
    pad_x: float = 18.0           # запас ширины текста в фигуре
    pad_y: float = 14.0
    pitch: float = 18.0           # межстрочный шаг текста
    max_chars: int = 30           # перенос длинных строк в блоках
    cond_chars: int = 22          # перенос строк в условии ромба

    # зазоры
    vgap: float = 30.0            # зазор между блоками основной линии
    hgap: float = 30.0            # ромб -> первая колонка веток
    colgap: float = 14.0          # зазор между соседними колонками
    mgap: float = 24.0            # плитки -> линия слияния
    jog: float = 6.0              # уступ линии при обходе плитки

    # рельсы «-> конец»
    rail: float = 14.0            # отступ рельсы от края колонки
    rail_step: float = 18.0       # шаг между рельсами разных веток

    # фигуры и лист
    term_round: float = 14.0      # скругление углов начала/конца
    conn_r: float = 15.0          # радиус кружка-соединителя
    conn_from_end: float = 22.0   # кружок у «конца»: отступ от края
    conn_step: float = 10.0       # шаг между кружками у «конца»
    aspect: float = 0.36          # отношение высоты ромба к ширине
    a4_w: float = 468.0           # рабочая ширина А4 вертикально (пункты)
    a4_h: float = 700.0           # рабочая высота А4 вертикально
    page_pad: float = 14.0        # поля страницы
    split_scale: float = 0.70     # режем на листы, лишь бы не мельче этого

    # качество и подписи
    dpi: int = 200                # плотность пикселей
    edge_lw: float = 1.4          # толщина всех линий и рамок (единая)
    label_dx: float = 8.0         # отступ подписи кейса от линии спуска
    label_dy: float = 12.0        # подпись: над плиткой / под углом ромба
    label_axis_dx: float = 9.0    # подпись среднего кейса: от осевой линии
    vertex_label_dy: float = 11.0 # подпись ветки «если»: над выходом ромба
    label_gap: float = 16.0       # шаг между подписями пустых веток
    letters: str = "АБВГДЕЖЗИКЛМНОПРСТУФХЦЧШЭЮЯ"  # кружки-соединители

    # ввод-вывод: вызовы, начинающиеся с этих слов, становятся
    # параллелограммами ввода-вывода
    io_words: tuple = ("printf", "scanf", "scan", "print", "println", "puts",
                       "putchar", "echo", "getchar", "gets", "cin", "cout",
                       "read", "write")

    def io_re(self):
        return re.compile(r"^(?:" + "|".join(self.io_words) + r")\b")


DEFAULT = Style()

# языки подписей и ключевых слов
LANGS = {
    "ru": {"start": "начало", "end": "конец", "yes": "да", "no": "нет"},
    "en": {"start": "Start", "end": "End", "yes": "yes", "no": "no"},
}

# алиасы для обратной совместимости (старый код и самотест)
FONT = DEFAULT.font
CHAR_W = DEFAULT.char_w
PAD_X = DEFAULT.pad_x
PAD_Y = DEFAULT.pad_y
PITCH = DEFAULT.pitch
VGAP = DEFAULT.vgap
HGAP = DEFAULT.hgap
COLGAP = DEFAULT.colgap
RAIL = DEFAULT.rail
RAIL_STEP = DEFAULT.rail_step
JOG = DEFAULT.jog
MGAP = DEFAULT.mgap
TERM_ROUND = DEFAULT.term_round
CONN_R = DEFAULT.conn_r
MAX_CHARS = DEFAULT.max_chars
COND_CHARS = DEFAULT.cond_chars
A4_W = DEFAULT.a4_w
A4_H = DEFAULT.a4_h
PAGE_PAD = DEFAULT.page_pad
ASPECT = DEFAULT.aspect
SPLIT_SCALE = DEFAULT.split_scale
DPI = DEFAULT.dpi
EDGE_LW = DEFAULT.edge_lw
FONT_STACK = list(DEFAULT.font_stack)
LETTERS = DEFAULT.letters
IO_RE = DEFAULT.io_re()
TEMPLATE = """\
#gostpadi 1
@labels en
# One line = one block; top to bottom. Five words: input, output, if, yes/no.
input scanf("%d", &a)
c = a * 2
if c > 10
    yes: printf("many"); break
    no: c = 0
output printf("c = %d", c)
"""


class ParseError(Exception):
    pass


class Node:
    def __init__(self, kind, text, branches=None, lang="en"):
        self.kind = kind                  # term | io | act | if | conn
        self.text = text
        self.branches = branches or []    # [(метка, [инструкции], to_end, link)]
        self.lang = lang                  # "en" | "ru" — язык ключевого слова


class Scheme:
    """Схема из кода: описываете текст и порядок, раскладка — сама."""

    def __init__(self):
        self.lines = []

    def raw(self, line):
        self.lines.append(line)
        return self

    def action(self, text):
        return self.raw(text)

    def input(self, text):
        return self.raw("input " + text)

    def output(self, text):
        return self.raw("output " + text)

    def branch(self, cond, **labels):
        self.raw("if " + cond)
        for label, value in labels.items():
            self.raw("    " + label + (": " + value if value else ":"))
        return self

    def text(self):
        return "\n".join(self.lines)

    def render(self, out_png):
        return render(self.text(), out_png)


# ---------- разбор формата ----------

def wrap(text, limit=MAX_CHARS):
    """Перенос длинных строк по пробелам (длинные токены режутся)."""
    res = []
    for para in text.split("\n"):
        while len(para) > limit:
            cut = para.rfind(" ", 0, limit)
            if cut <= 0:
                cut = limit
            res.append(para[:cut].rstrip())
            para = para[cut:].lstrip()
        res.append(para)
    return "\n".join(res)


def split_statements(text):
    """'printf(...); return 1' -> ['printf(...)', 'return 1'] — плитки колонки."""
    parts, cur, depth, in_str = [], [], 0, False
    for ch in text:
        if ch == '"' and (not cur or cur[-1] != "\\"):
            in_str = not in_str
        if not in_str:
            if ch in "([{":
                depth += 1
            elif ch in ")]}":
                depth = max(0, depth - 1)
            elif ch == ";" and depth == 0:
                parts.append("".join(cur))
                cur = []
                continue
        cur.append(ch)
    parts.append("".join(cur))
    return [p.strip() for p in parts if p.strip()]


def cond_text(cond):
    cond = cond.strip()
    if re.match(r"^(if|switch)\b", cond):
        return cond
    return f"if ({cond})"


def strip_comments(line):
    """Убирает комментарий # или // (вне кавычек строки)."""
    out, in_str = [], False
    for i, ch in enumerate(line):
        if ch == '"' and (i == 0 or line[i - 1] != "\\"):
            in_str = not in_str
            out.append(ch)
        elif not in_str and ch == "#":
            break
        elif not in_str and ch == "/" and line[i + 1:i + 2] == "/":
            break
        else:
            out.append(ch)
    return "".join(out)


def parse(text, st=DEFAULT):
    """Текст схемы (.gvn) -> список узлов (начало/конец добавляются сами)."""
    text = text.lstrip("\ufeff")  # BOM
    lines = []
    for lineno, line in enumerate(text.splitlines(), 1):
        body = strip_comments(line).expandtabs(4).rstrip()
        if body.strip():
            lines.append((lineno, body))

    nodes = [Node("term", "начало", lang="ru")]
    labels_lang = "ru"  # язык надписей «начало»/«конец»: @labels en|ru
    i = 0
    while i < len(lines):
        lineno, line = lines[i]
        stripped = line.strip()
        indent = len(line) - len(line.lstrip())
        if stripped.startswith("@") and indent == 0:
            m = re.match(r"^@labels\s+(ru|en)\s*$", stripped)
            if not m:
                raise ParseError(f"строка {lineno}: неизвестная директива "
                                 f"«{stripped}» (есть только @labels ru|en)")
            labels_lang = m.group(1)
            i += 1
            continue
        if indent >= 4:
            raise ParseError(f"строка {lineno}: отступ допустим только внутри "
                             "«если»/«if»")
        if re.match(r"^(если|if|switch)[ (]", stripped):
            # ромб: «если усл.» / «if усл.» / голый «switch (...)»
            kw = stripped.split("(", 1)[0].split(" ", 1)[0]
            lang = "ru" if kw == "если" else "en"
            rest = stripped[len(kw):].strip()
            cond = wrap(("switch " + rest) if kw == "switch"
                        else cond_text(rest), st.cond_chars)
            branches = []
            j = i + 1
            while j < len(lines):
                jline = lines[j][1]
                if len(jline) - len(jline.lstrip()) < 4:
                    break
                jline = jline.strip()
                m1 = re.match(r"(.+?)\s*->\s*(?:конец|end)\s*:\s*(.*)$", jline)
                m2 = re.match(r"(.+?)\s*:\s*(.*)$", jline)
                if m1:
                    label, to_end, btext = m1.group(1).strip(), True, m1.group(2).strip()
                elif m2:
                    label, to_end, btext = m2.group(1).strip(), False, m2.group(2).strip()
                else:
                    raise ParseError(
                        f"строка {lines[j][0]}: ветка должна быть вида "
                        "«метка: текст»")
                for suf in ("-> конец", "-> end"):
                    if btext.endswith(suf):
                        btext = btext[:-len(suf)].rstrip()
                        to_end = True
                branches.append((label, split_statements(btext), to_end, None))
                j += 1
            if not branches:
                raise ParseError(f"строка {lineno}: у «{kw}» нет ни одной ветки")
            if len(branches) == 1 and branches[0][1] and not cond.startswith("switch"):
                # у одиночного условия второй путь рисуем сами
                branches.append(("no" if labels_lang == "en" else "нет",
                                 [], False, None))
            nd = Node("if", cond, branches, lang=lang)
            m = re.match(r"switch\s*\(([^)]*)\)", cond)
            nd.switch_var = m.group(1).strip() if m else None
            nodes.append(nd)
            i = j
            continue
        m_in = re.match(r"^(?:ввод|input)\s+(?![=+\-*/])", stripped)
        m_out = re.match(r"^(?:вывод|output)\s+(?![=+\-*/])", stripped)
        m_act = re.match(r"^(?:действие|action)\s+", stripped)
        if m_in:
            nodes.append(Node("io", wrap(stripped[m_in.end():].strip()), lang="ru"))
        elif m_out:
            nodes.append(Node("io", wrap(stripped[m_out.end():].strip())))
        elif m_act:
            nodes.append(Node("act", wrap(stripped[m_act.end():].strip()),
                              lang="ru"))
        elif IO_RE.match(stripped):
            nodes.append(Node("io", wrap(stripped)))
        else:
            nodes.append(Node("act", wrap(stripped)))
        i += 1
    nodes.append(Node("term", "", lang=labels_lang))
    st, fin = (("Start", "End") if labels_lang == "en"
               else ("начало", "конец"))
    nodes[0].text = st
    nodes[-1].text = fin
    return nodes


# ---------- измерение: один тип фигур — один размер ----------

def measure(st, kind, text):
    """(ширина, высота) фигуры по её тексту."""
    ls = text.split("\n")
    tw = max(len(l) for l in ls) * st.char_w
    n = len(ls)
    if kind == "term":
        return max(tw + st.pad_x + 22.0, 78.0), max(n * st.pitch * 0.8 + 16.0, 36.0)
    if kind == "conn":
        return 2 * st.conn_r, 2 * st.conn_r
    if kind == "if":
        # текст помещается между рёбрами ромба на глубине текста,
        # пропорции остаются ромбом (~1 : 2.8), а не приплюснутым
        # четырёхугольником; закрутная формула: h = st.aspect * w
        ymax = (n - 1) * st.pitch / 2 + 6.0
        need = tw + 26.0
        w = max(need + ymax * 2.0 / st.aspect, 150.0)
        h = max(st.aspect * w, 2 * ymax + 28.0, 44.0)
        return w, h
    return tw + st.pad_x + 6.0, n * st.pitch + st.pad_y - 4.0


def normalize(nodes, st=DEFAULT):
    """Максимальный размер на каждый тип фигур — единый для всей схемы."""
    sizes = {}

    def put(kind, t):
        w, h = measure(st, kind, t)
        if kind in sizes:
            sizes[kind] = (max(sizes[kind][0], w), max(sizes[kind][1], h))
        else:
            sizes[kind] = (w, h)

    for nd in nodes:
        put(nd.kind, nd.text)
        for _lbl, stmts, _te, _ln in nd.branches:
            for s in stmts:
                put("io" if IO_RE.match(s) else "act", s)
    # базовые размеры всегда есть, даже если типа в схеме не было
    # (иначе раскладка падает на отсутствующем ключе)
    for kind, sample in (("term", "x"), ("io", "x"), ("act", "x"),
                         ("if", "x"), ("conn", "А")):
        put(kind, sample)
    return sizes


# ---------- раскладка ----------

def layout(nodes, sizes, st=DEFAULT):
    """Схема -> (фигуры, рёбра, подписи, границы, якоря узлов) в пунктах.

    Фигура: dict(kind, cx, cy, w, h, lines).
    Ребро:  dict(points=[(x, y), ...], arrow=bool) — ломаная из сегментов
            строго 90°; стрелка ставится в конце, если arrow.
    Якоря:  главная фигура каждого узла (для разбивки схемы на части).
    """
    shapes, edges, labels, anchors = [], [], [], []
    colw = max(sizes["act"][0], sizes["io"][0])
    pend = []  # ветки «-> конец»: ждут блока «конец»
    pend_count = {"L": 0, "R": 0}  # рельсы «-> конец» гнездятся снаружи
    # самая широкая раскладка веток по всей схеме: рельсы «-> конец» идут
    # за колонками всех ромбов, чтобы не задевать чужие плитки на спуске
    max_tier = 0
    for scan in nodes:
        if scan.kind == "if":
            m = sum(1 for b in scan.branches if b[1])
            max_tier = max(max_tier, m // 2 - 1 if m >= 2 else 0)

    def add(kind, cx, cy, text):
        w, h = sizes[kind]
        sh = dict(kind=kind, cx=cx, cy=cy, w=w, h=h, lines=text.split("\n"))
        shapes.append(sh)
        return sh

    def edge(pts, arrow=True):
        clean = [pts[0]]
        for p in pts[1:]:
            if p != clean[-1]:
                clean.append(p)
        if len(clean) >= 2:
            edges.append(dict(points=clean, arrow=arrow))

    def simple(nd, prev, cursor):
        w, h = sizes[nd.kind]
        cy = cursor + st.vgap + h / 2
        sh = add(nd.kind, 0.0, cy, nd.text)
        if prev is not None:
            edge([prev, (0.0, cy - h / 2)])
        anchors.append(dict(sh=sh, ext=cy + h / 2))
        return (0.0, cy + h / 2), cy + h / 2, sh

    prev, cursor = None, 0.0
    last = len(nodes) - 1
    for idx, nd in enumerate(nodes):
        # ---------- простой блок на основной линии ----------
        if nd.kind != "if":
            if idx == last and pend:
                cursor = max(cursor, max(p["y"] for p in pend))
            prev, cursor, sh = simple(nd, prev, cursor)
            if idx == last:
                stop_l = sh["cx"] - sh["w"] / 2
                stop_r = sh["cx"] + sh["w"] / 2
            if idx == last and pend:
                for p in pend:
                    ex = stop_l if p["rail"] < 0 else stop_r
                    edge([(p["x"], p["y"]), (p["x"], p["cb"] + st.jog),
                          (p["rail"], p["cb"] + st.jog), (p["rail"], sh["cy"]),
                          (ex, sh["cy"])])
            if idx == last and getattr(nd, "inbound", None):
                # кружки-соединители «-> конец» с предыдущих листов
                for i, letter in enumerate(nd.inbound):
                    cx_ = (sh["cx"] - sh["w"] / 2 - 2 * st.conn_r - 22
                           - i * (2 * st.conn_r + 10))
                    add("conn", cx_, sh["cy"], letter)
                    edge([(cx_ + st.conn_r, sh["cy"]), (stop_l, sh["cy"])])
            continue

        # ---------- ромб «если» ----------
        dw, dh = sizes["if"]
        cy = cursor + st.vgap + dh / 2
        dmd = add("if", 0.0, cy, nd.text)
        if prev is not None:
            edge([prev, (0.0, cy - dh / 2)])
        anchors.append(dict(sh=dmd, ext=0.0))  # дополнится низом колонок
        vl, vr, vb = (-dw / 2, cy), (dw / 2, cy), (0.0, cy + dh / 2)

        svar = getattr(nd, "switch_var", None)
        nonempty = [b for b in nd.branches if b[1]]
        n = len(nonempty)
        comb = bool(svar) and n >= 2
        # у switch линия раздачи кейсов лежит прямо на нижнем углу ромба
        # (проходит через его вершину), и все кейсы висят на ней
        top0 = cy + dh / 2 + st.vgap
        y_b = cy + dh / 2

        def fmt(label):
            """Ветка switch: «1» -> «status = 1», «иначе» -> «default»."""
            if svar:
                if label.strip().lower() in ("иначе", "default", "else",
                                             "otherwise"):
                    return "default"
                if "=" not in label:
                    return f"{svar} = {label}"
            return label

        empty = [fmt(b[0]) for b in nd.branches if not b[1]]
        pitch = colw + st.colgap
        base = dw / 2 + st.hgap + colw / 2
        # колонка на каждую ветку: средняя — прямо по оси от нижней вершины,
        # соседние — от боковых вершин, дальние — ещё одной линией под 90°
        plan = []  # (ветка, side, tier, tx)
        if n == 1 and empty:
            plan.append((nonempty[0], "L", 0, -base))
        else:
            for j, b in enumerate(nonempty):
                if n % 2 and j == n // 2:
                    plan.append((b, "axis", 0, 0.0))
                elif j < n // 2:
                    t = n // 2 - 1 - j
                    plan.append((b, "L", t, -(base + t * pitch)))
                elif n:
                    t = j - n // 2 - (n % 2)
                    plan.append((b, "R", t, base + t * pitch))

        exits = {}  # id(ветки) -> dict(x, y, to_end, side)
        link_bottom = cy + dh / 2
        # switch: из нижнего угла ромба выходит горизонтальная линия во всю
        # ширину колонок (проходит ровно через угол), и все кейсы висят
        # на ней вертикальными спусками
        if comb and len(plan) >= 2:
            xs = [tx for _b, _s, _t, tx in plan]
            edge([(min(xs), y_b), (max(xs), y_b)], arrow=False)
        for b, side, _t, tx in plan:
            label, stmts, to_end, link = b
            prev_bottom = None
            for si, s in enumerate(stmts):
                k = "io" if IO_RE.match(s) else "act"
                w_k, h_k = sizes[k]
                top = top0 if prev_bottom is None else prev_bottom + st.vgap
                bottom = top + h_k
                sh_k = add(k, tx, top + h_k / 2, s)
                if si == 0:
                    if side == "axis":
                        # средний кейс: спуск прямо из нижнего угла ромба
                        edge([(tx, y_b), (tx, top)])
                        labels.append(dict(x=9.0, y=cy + dh / 2 + 12.0,
                                           text=fmt(label), ha="left"))
                    elif comb:
                        # кейс висит на горизонтальной линии угла
                        edge([(tx, y_b), (tx, top)])
                        labels.append(dict(
                            x=tx + (-8.0 if side == "L" else 8.0),
                            y=top - 12.0, text=fmt(label),
                            ha="right" if side == "L" else "left"))
                    else:
                        v = vl if side == "L" else vr
                        edge([v, (tx, cy), (tx, top)])
                        labels.append(dict(x=(v[0] + tx) / 2, y=cy - 11.0,
                                           text=fmt(label), ha="center"))
                else:
                    edge([(tx, prev_bottom), (tx, top)])
                prev_bottom = bottom
            exits[id(b)] = dict(x=tx, y=prev_bottom, to_end=to_end, side=side)

        # слияние колонок обратно на основную линию
        merge_y = cy + dh / 2
        for b, _s, _t, _x in plan:
            if not exits[id(b)]["to_end"]:
                merge_y = max(merge_y, exits[id(b)]["y"] + st.mgap)
        col_bottom = max([e["y"] for e in exits.values()] or [cy + dh / 2])
        for b, side, _t, _x in plan:
            e = exits[id(b)]
            if e["to_end"]:  # «-> конец» — ждём блока конца
                # рельса снаружи колонок всех ромбов схемы: спуск до «конца»
                # не касается ни плиток, ни линий подачи, ни линий слияния
                sgn, key = (-1.0, "L") if side == "L" else (1.0, "R")
                rail = sgn * (base + max_tier * pitch + colw / 2 + st.rail +
                              pend_count[key] * st.rail_step)
                pend_count[key] += 1
                if link:
                    # «конец» на другом листе: ветка кончается кружком-
                    # соединителем с той же буквой у «конца»
                    ccy = col_bottom + st.jog + st.vgap + st.conn_r
                    add("conn", rail, ccy, link)
                    edge([(e["x"], e["y"]), (e["x"], col_bottom + st.jog),
                          (rail, col_bottom + st.jog), (rail, ccy - st.conn_r)])
                    link_bottom = ccy + st.conn_r
                else:
                    pend.append(dict(x=e["x"], y=e["y"], cb=col_bottom,
                                     rail=rail))
            else:
                edge([(e["x"], e["y"]), (e["x"], merge_y), (0.0, merge_y)],
                     arrow=False)
        has_axis = any(p[1] == "axis" for p in plan)
        has_merges = any(not e["to_end"] for e in exits.values())
        # по ГОСТ у решения оба выхода — из боковых вершин: пустая ветка
        # (нет/иначе) идёт обходом справа и сливается на основной линии,
        # от нижней вершины ромба ничего не выходит
        if empty:
            merge_y = max(merge_y, cy + dh / 2 + 18.0)
        for k, lbl in enumerate(empty):
            bx = dw / 2 + 24.0 + max_tier * pitch + colw + k * 20.0
            edge([vr, (bx, cy), (bx, merge_y), (0.0, merge_y)], arrow=False)
            labels.append(dict(x=dw / 2 + 10.0, y=cy - 11.0, text=lbl,
                               ha="left"))
        if not has_merges and not empty and not has_axis:
            edge([vb, (0.0, merge_y)], arrow=False)
        prev = (0.0, merge_y)
        cursor = max(merge_y, col_bottom, link_bottom)
        anchors[-1]["ext"] = cursor

    # границы
    xs, ys = [], []
    for e in edges:
        for p in e["points"]:
            xs.append(p[0])
            ys.append(p[1])
    for sh in shapes:
        xs += [sh["cx"] - sh["w"] / 2, sh["cx"] + sh["w"] / 2]
        ys += [sh["cy"] - sh["h"] / 2, sh["cy"] + sh["h"] / 2]
    for l in labels:
        half = len(l["text"]) * 4.8
        if l["ha"] == "right":
            xs.append(l["x"] - half)
        elif l["ha"] == "left":
            xs.append(l["x"] + half)
        else:
            xs += [l["x"] - half, l["x"] + half]
        ys.append(l["y"])
    pad = st.page_pad
    minx, miny = min(xs) - pad, min(ys) - pad
    return (shapes, edges, labels,
            (minx, miny, max(xs) - minx + 2 * pad, max(ys) - miny + 2 * pad),
            anchors)


def split_scheme(nodes, sizes, st=DEFAULT):
    """Не влезает в А4 -> части, соединённые кружками «А», «Б», ..."""
    items = list(nodes)
    parts = []
    li = 0
    while True:
        result = layout(items, sizes)
        bounds = result[3]
        # лёгкое уменьшение лучше разрыва схемы: режем только если без
        # разбиения масштаб упал бы ниже st.split_scale
        if bounds[3] <= (st.a4_h - 2 * st.page_pad) / st.split_scale or len(items) < 6:
            parts.append(items)
            break
        limit = (st.a4_h - 2 * st.page_pad) * 0.88
        cut = None
        for j, a in enumerate(result[4]):
            if j < 2 or j > len(items) - 3:
                continue
            # резать можно перед простым блоком или перед «если»
            # (ромб с колонками целиком остаётся в предыдущей части)
            if items[j].kind not in ("act", "io", "if"):
                continue
            if result[4][j - 1]["ext"] <= limit:
                cut = j
        if cut is None:
            parts.append(items)
            break
        letter = st.letters[li % len(st.letters)]
        li += 1
        parts.append(items[:cut] + [Node("conn", letter)])
        items = [Node("conn", letter)] + items[cut:]
    # ветки «-> конец» в не-последних частях кончаются кружком-соединителем:
    # сам «конец» живёт на последнем листе, туда же ставятся его кружки
    links = []
    for part in parts[:-1]:
        for nd in part:
            if nd.kind != "if":
                continue
            for bi, br in enumerate(nd.branches):
                if br[2] and not br[3]:
                    letter = st.letters[li % len(st.letters)]
                    li += 1
                    nd.branches[bi] = (br[0], br[1], True, letter)
                    links.append(letter)
    parts[-1][-1].inbound = sorted(links)
    return parts


# ---------- рендер ----------

def _draw_shape(ax, sh, s, ox, oy, fs, lw, st):
    X = lambda x: (x - ox) * s
    Y = lambda y: (y - oy) * s
    cx, cy = X(sh["cx"]), Y(sh["cy"])
    w, h = sh["w"] * s, sh["h"] * s
    k = sh["kind"]
    if k == "term":
        ax.add_patch(FancyBboxPatch(
            (cx - w / 2, cy - h / 2), w, h,
            boxstyle=f"round,pad=0,rounding_size={min(st.term_round * s, h / 2.4)}",
            fill=True, facecolor="white", edgecolor="black", lw=lw))
    elif k == "io":
        skew = h * 0.3
        x0, y0, x1, y1 = cx - w / 2, cy - h / 2, cx + w / 2, cy + h / 2
        ax.add_patch(Polygon([(x0 + skew, y0), (x1, y0), (x1 - skew, y1), (x0, y1)],
                             closed=True, fill=True, facecolor="white",
                             edgecolor="black", lw=lw))
    elif k == "if":
        ax.add_patch(Polygon([(cx - w / 2, cy), (cx, cy - h / 2),
                              (cx + w / 2, cy), (cx, cy + h / 2)],
                             closed=True, fill=True, facecolor="white",
                             edgecolor="black", lw=lw))
    elif k == "conn":
        ax.add_patch(Circle((cx, cy), CONN_R * s, fill=True, facecolor="white",
                            edgecolor="black", lw=lw))
    else:
        ax.add_patch(Rectangle((cx - w / 2, cy - h / 2), w, h, fill=True,
                               facecolor="white", edgecolor="black", lw=lw))
    if k == "conn":
        ax.text(cx, cy, "\n".join(sh["lines"]), ha="center", va="center",
                fontsize=fs * 0.95, family=st.font_stack, weight="bold",
                color="black")
    else:
        ax.text(cx, cy, "\n".join(sh["lines"]), ha="center", va="center",
                fontsize=fs, family=st.font_stack, linespacing=1.25,
                color="black")


def _draw_edge(ax, e, s, ox, oy, lw=EDGE_LW):
    pts = [((x - ox) * s, (y - oy) * s) for x, y in e["points"]]
    tail = pts if not e.get("arrow", True) else pts[:-1]
    if len(tail) >= 2:
        xs, ys = zip(*tail)
        ax.plot(xs, ys, color="black", lw=lw, solid_capstyle="projecting",
                solid_joinstyle="miter")
    if e.get("arrow", True):
        a, b = pts[-2], pts[-1]
        ax.add_patch(FancyArrowPatch(a, b, arrowstyle="-|>",
                                     mutation_scale=max(6.0, 14.0 * s),
                                     color="black", lw=lw, shrinkA=0,
                                     shrinkB=0))


def draw(shapes, edges, labels, bounds, out_png, page="a4", scale=None,
         font=None, edge_lw=None, dpi=None, st=DEFAULT):
    """Раскладка -> PNG. page="a4" вписывает в А4 вертикально (165x247 мм),
    page="auto" оставляет канвас по размеру контента (масштаб 1:1),
    scale задаёт масштаб вручную, font — базовый кегль, edge_lw — толщина
    всех линий и рамок (по умолчанию единая EDGE_LW), dpi — плотность."""
    lw = edge_lw if edge_lw else st.edge_lw
    font = font if font is not None else st.font
    dpi = dpi if dpi else st.dpi
    dpi = dpi or DPI
    minx, miny, W, H = bounds
    if scale is not None:
        s = scale
    elif page == "auto":
        # оптимально: 1:1, если влезает; сверх А4 — ужимаем, но не режем
        s = min(1.0, (A4_W - 2 * PAGE_PAD) / W, (A4_H - 2 * PAGE_PAD) / H)
    else:
        s = min(1.0, (A4_W - 2 * PAGE_PAD) / W, (A4_H - 2 * PAGE_PAD) / H)
    fig = plt.figure(figsize=(W * s / 72.0, H * s / 72.0),
                     dpi=dpi / max(s, 1e-6))
    ax = fig.add_axes([0, 0, 1, 1])
    ax.set_xlim(0, W * s)
    ax.set_ylim(H * s, 0)
    ax.axis("off")
    fs = font * s
    for sh in shapes:
        _draw_shape(ax, sh, s, minx, miny, fs, lw, st)
    for e in edges:
        _draw_edge(ax, e, s, minx, miny, lw)
    for l in labels:
        ax.text((l["x"] - minx) * s, (l["y"] - miny) * s, l["text"],
                fontsize=max(4.5, fs * 0.85), family="DejaVu Sans",
                weight="bold", ha=l["ha"], va="center", color="black")
    fig.savefig(out_png, dpi=dpi / max(s, 1e-6), facecolor="white")
    plt.close(fig)


def _suffixed(path, n):
    root, dot, ext = str(path).rpartition(".")
    return f"{root}-{n}.{ext}" if dot else f"{path}-{n}"


def render(text, out_png, page="a4", scale=None, font=FONT, edge_lw=None,
           dpi=DPI, c_labels="ru"):
    """Текст схемы -> PNG (или несколько: результат.png, результат-2.png...).

    page="a4" — вписать в А4 (по умолчанию), page="auto" — канвас по
    контенту без ужимания, scale — принудительный масштаб, font — кегль,
    edge_lw — единая толщина линий, dpi — плотность пикселей, c_labels —
    язык подписей при чтении кода C (да/нет или yes/no).
    Возвращает список записанных файлов.
    """
    nodes = parse(text)
    sizes = normalize(nodes)
    files = []
    parts = [nodes] if page == "auto" else split_scheme(nodes, sizes)
    for k, part in enumerate(parts):
        target = out_png if k == 0 else _suffixed(out_png, k + 1)
        draw(*layout(part, sizes)[:4], target, page=page, scale=scale,
             font=font, edge_lw=edge_lw, dpi=dpi)
        files.append(target)
    return files


# ---------- C -> .gvn ----------

C_TYPE_RE = re.compile(r"^(?:(?:unsigned|signed|long|short)\s+)*"
                       r"(?:int|float|double|char)\b")


def _strip_c_comments(src):
    """Убирает /* */ и // из кода C, строковые литералы не трогает."""
    out, i, n, in_str = [], 0, len(src), False
    while i < n:
        c = src[i]
        if in_str:
            out.append(c)
            if c == "\\" and i + 1 < n:
                out.append(src[i + 1])
                i += 2
                continue
            if c == '"':
                in_str = False
            i += 1
            continue
        if c == '"':
            in_str = True
            out.append(c)
            i += 1
            continue
        if c == "/" and src[i + 1:i + 2] in ("/", "*"):
            if src[i + 1] == "/":
                j = src.find("\n", i)
                i = n if j < 0 else j
            else:
                j = src.find("*/", i + 2)
                i = n if j < 0 else j + 2
                out.append(" ")
            continue
        out.append(c)
        i += 1
    return "".join(out)


def _clean_stmt(text):
    """Инструкция: схлопнуть пробелы; объявление без '=' выбросить,
    объявление с инициализацией превратить в присваивание."""
    text = re.sub(r"\s+", " ", text).strip().rstrip(";").strip()
    m = re.match(r"(?:(?:unsigned|signed|long|short)\s+)*"
                 r"(?:int|float|double|char)\s+(.*)$", text)
    if m:
        rest = m.group(1).strip()
        return rest if "=" in rest else None
    return text or None


class _CSrc:
    def __init__(self, s):
        self.s, self.i, self.n = s, 0, len(s)

    def skip_ws(self):
        while self.i < self.n and self.s[self.i] in " \t\r\n":
            self.i += 1

    def at_word(self, w):
        if not self.s.startswith(w, self.i):
            return False
        after = self.s[self.i + len(w):self.i + len(w) + 1]
        return after not in "abcdefghijklmnopqrstuvwxyz" \
                             "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"


def _cs_at_word(s, w):
    if not s.s.startswith(w, s.i):
        return False
    after = s.s[s.i + len(w):s.i + len(w) + 1]
    return after not in "abcdefghijklmnopqrstuvwxyz" \
                        "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"


def _cs_read_paren(s):
    """От '(' до парной ')': возвращает текст внутри."""
    s.skip_ws()
    if s.s[s.i] != "(":
        raise ParseError("в коде ожидалась (")
    depth, i = 0, s.i
    while i < s.n:
        c = s.s[i]
        if c == '"':
            i += 1
            while i < s.n and s.s[i] != '"':
                i += 2 if s.s[i] == "\\" else 1
            i += 1
            continue
        if c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth == 0:
                inner = s.s[s.i + 1:i]
                s.i = i + 1
                return inner
        i += 1
    raise ParseError("в коде не закрыта скобка (")


def _cs_read_simple(s):
    """Инструкция до ';' на нулевой глубине (строки и скобки внутри)."""
    depth, i, start = 0, s.i, s.i
    while i < s.n:
        c = s.s[i]
        if c == '"':
            i += 1
            while i < s.n and s.s[i] != '"':
                i += 2 if s.s[i] == "\\" else 1
            i += 1
            continue
        if c in "([":
            depth += 1
        elif c in ")]":
            depth -= 1
        elif c == ";" and depth == 0:
            break
        i += 1
    text = s.s[start:i]
    s.i = i + 1 if i < s.n else i
    return text


def _shorten_calls(cond):
    """scanf("%d", &x) -> scanf(...) — форматные строки в условиях не нужны."""
    return re.sub(r'(\w+)\("[^"]*"[^)]*\)', r"\1(...)", cond)


def _c_statement(s):
    """Инструкция C -> ("stmt", текст) | ("if", условие, да, нет) |
    ("switch", выражение, [(метка, инструкции)])."""
    s.skip_ws()
    if s.at_word("if"):
        s.i += 2
        cond = _shorten_calls(_cs_read_paren(s))
        s.skip_ws()
        if s.s[s.i] == "{":
            s.i += 1
            yes = _c_parse_block(s)
        else:
            yes = [_c_statement(s)]
        s.skip_ws()
        no = []
        if s.at_word("else"):
            s.i += 4
            s.skip_ws()
            if s.at_word("if"):
                raise ParseError("в коде else if — .gvn не вкладывает ромбы "
                                 "друг в друга, разбей условие")
            if s.s[s.i] == "{":
                s.i += 1
                no = _c_parse_block(s)
            else:
                no = [_c_statement(s)]
        return ("if", cond, yes, no)
    if s.at_word("switch"):
        s.i += 6
        expr = _cs_read_paren(s)
        s.skip_ws()
        if s.s[s.i] != "{":
            raise ParseError("в коде после switch (...) ожидалась {")
        s.i += 1
        cases = []
        while True:
            s.skip_ws()
            if s.i >= s.n:
                raise ParseError("в коде не закрыта скобка {")
            if s.s[s.i] == "}":
                s.i += 1
                return ("switch", expr, cases)
            m = re.match(r"case\s+([^:]+):", s.s[s.i:])
            if m:
                s.i += m.end()
                cases.append((m.group(1).strip(), []))
                continue
            if re.match(r"default\s*:", s.s[s.i:]):
                s.i += s.s[s.i:].find(":") + 1
                cases.append(("default", []))
                continue
            if not cases:
                raise ParseError("в switch инструкция до первой метки case")
            cases[-1][1].append(_c_statement(s))
    if s.at_word("while") or s.at_word("for") or s.at_word("do"):
        raise ParseError("в коде цикл (while/for/do) — циклы пока не "
                         "поддерживаются")
    if s.at_word("goto"):
        raise ParseError("в коде goto — не поддерживается")
    text = _clean_stmt(_cs_read_simple(s))
    return ("skip",) if text is None else ("stmt", text)


def _c_parse_block(s):
    """Инструкции до закрывающей } (её съедает) или до конца ввода."""
    items = []
    while True:
        s.skip_ws()
        if s.i >= s.n:
            raise ParseError("в коде не закрыта скобка {")
        if s.s[s.i] == "}":
            s.i += 1
            return items
        items.append(_c_statement(s))


def _branch_text(items):
    """Инструкции ветки -> строка через ';' (плитки колонки)."""
    parts = []
    for it in items:
        if it[0] == "stmt":
            parts.append(_abbrev_stmt(it[1]))
        elif it[0] != "skip":
            raise ParseError("вложенный if/switch внутри ветки — пока не "
                             "поддерживается, вынеси его на верхний уровень")
    return "; ".join(parts)


def _has_return(items):
    return any(it[0] == "stmt" and it[1].lower().startswith("return")
               for it in items)



def _abbrev_stmt(s):
    """Длинные printf("…") сокращаем до printf("Начало фразы...") —
    как принято в учебных схемах; условия и присваивания не трогаем."""
    if len(s) <= 26:
        return s
    m = re.match(r'^(printf|puts|print|echo|write)\("(.*)"\s*(,.*)?\)$', s)
    if not m:
        return s
    content = m.group(2)
    cut = content[:15]
    if " " in cut:
        cut = cut[:cut.rfind(" ")]
    return f'{m.group(1)}("{cut}...")'


def c_to_gvn(src, labels="ru"):
    """Код C -> текст схемы .gvn. if/else, switch/case/default, return;
    объявления переменных выбрасываются, ветка с return уходит в «конец».
    Циклы (while/for/do) пока не поддерживаются."""
    src = _strip_c_comments(src)
    m = re.search(r"\bmain\s*\([^)]*\)\s*\{", src)
    if not m:
        raise ParseError("в коде не найден int main(...)")
    s = _CSrc(src)
    s.i = m.end()
    items = [it for it in _c_parse_block(s) if it[0] != "skip"]
    tag_yes, tag_no = ("yes", "no") if labels == "en" else ("да", "нет")
    lines = ["#gostpadi 1"]
    if labels == "en":
        lines.append("@labels en")

    def emit(items):
        for it in items:
            if it[0] == "stmt":
                text = _abbrev_stmt(it[1])
                if not text.lower().startswith("return"):
                    lines.append(text)  # верхнеуровневый return не рисуем
            elif it[0] == "if":
                _, cond, yes, no = it
                lines.append("if " + cond)
                for tag, branch in ((tag_yes, yes), (tag_no, no)):
                    text = _branch_text(branch)
                    if text:
                        lines.append("    " + tag
                                     + (" -> end: " if _has_return(branch)
                                        else ": ") + text)
                if not _branch_text(no) and not _has_return(no):
                    lines.append("    " + tag_no + ":")
            elif it[0] == "switch":
                lines.append("switch (" + it[1] + ")")
                for label, case_items in it[2]:
                    text = _branch_text(case_items)
                    if text:
                        lines.append("    " + label
                                     + (" -> end: " if _has_return(case_items)
                                        else ": ") + text)

    emit(items)
    return "\n".join(lines) + "\n"


def render_file(in_path, out_png, **kw):
    """Файл схемы (.gvn) или кода C (.c) -> PNG.
    Возвращает (код возврата, список файлов)."""
    try:
        src = open(in_path, encoding="utf-8").read()
    except OSError as e:
        print(f"нет файла: {e}", file=sys.stderr)
        return 1, []
    try:
        text = (c_to_gvn(src, labels=kw.pop("c_labels", "ru"))
                if in_path.endswith(".c") else src)
        files = render(text, out_png, **kw)
    except ParseError as e:
        print(f"ошибка: {e}", file=sys.stderr)
        return 1, []
    except OSError as e:
        print(f"не удалось записать: {e}", file=sys.stderr)
        return 1, []
    for f in files:
        print(f)
    return 0, files


def _default_output(inp):
    root, _ = os.path.splitext(inp)
    return (root or inp) + ".png"


def _terminal_protocol():
    """Какой протокол графики поддерживает терминал (по окружению)."""
    force = os.environ.get("GOSTPADI_SHOW", "").strip().lower()
    if force in ("kitty", "iterm", "none"):
        return force
    marks = " ".join(filter(None, (os.environ.get("TERM", ""),
                                   os.environ.get("TERM_PROGRAM", ""),
                                   os.environ.get("LC_TERMINAL", "")))).lower()
    if ("kitty" in marks or "wezterm" in marks or "ghostty" in marks
            or os.environ.get("GHOSTTY_RESOURCES_DIR")):
        return "kitty"
    if "iterm" in marks or "wezterm" in marks:
        return "iterm"
    return "none"


def _show_in_terminal(png_path):
    """Печатает PNG прямо в терминал (kitty- или iTerm2-протокол)."""
    import base64
    from PIL import Image
    with open(png_path, "rb") as f:
        blob = f.read()
    w, h = Image.open(png_path).size
    cols = max(20, min(shutil.get_terminal_size((100, 40)).columns, 180))
    rows = max(6, round(cols * h / w / 2))  # ячейка терминала ~2:1
    b64 = base64.b64encode(blob).decode()
    proto = _terminal_protocol()
    if proto == "kitty":
        chunks = [b64[i:i + 4000] for i in range(0, len(b64), 4000)] or [""]
        parts = []
        for i, ch in enumerate(chunks):
            m = " m=1" if i < len(chunks) - 1 else " m=0"
            parts.append(f"\x1b_Gf=32,s={w},v={h},c={cols},r={rows}{m}\x1b\\{ch}")
        sys.stdout.write("".join(parts) + "\n")
        return True
    if proto == "iterm":
        head = (f"\x1b]1337;File=inline=1;size={len(blob)};"
                f"width={cols};height={rows}:")
        sys.stdout.write(head + b64 + "\x07\n")
        return True
    return False


def main(argv):
    args, output = [], None
    page, scale, font, lw, dpi = "a4", None, FONT, None, DPI
    show, template, gvn = False, False, False
    c_labels = "ru"
    i = 1
    while i < len(argv):
        a = argv[i]
        if a == "--auto":
            page = "auto"
        elif a == "--show":
            show = True
        elif a == "--template":
            template = True
        elif a == "--gvn":
            gvn = True
        elif a in ("-o", "--output"):
            i += 1
            if i >= len(argv):
                print(f"{a}: нужно имя файла", file=sys.stderr)
                return 2
            output = argv[i]
        elif a.startswith("--output="):
            output = a[9:]
        elif a.startswith("--scale="):
            scale = float(a[8:])
        elif a.startswith("--font="):
            font = float(a[7:])
        elif a.startswith("--labels="):
            c_labels = a[9:]
        elif a.startswith("--lw="):
            lw = float(a[5:])
        elif a.startswith("--dpi="):
            dpi = int(a[6:])
        elif a in ("-h", "--help"):
            print(__doc__)
            return 0
        elif a in ("-v", "--version"):
            print(f"gostpadi {__version__}")
            return 0
        elif a.startswith("-") and a != "-":
            print(f"неизвестная опция: {a}", file=sys.stderr)
            return 2
        else:
            args.append(a)
        i += 1

    for opt_name, val in (("--scale", scale), ("--font", font),
                          ("--lw", lw), ("--dpi", dpi)):
        if val is not None and val <= 0:
            print(f"{opt_name}={val}: значение должно быть > 0",
                  file=sys.stderr)
            return 2
    if template:
        sys.stdout.write(TEMPLATE)
        return 0
    if not args:
        print("использование: gostpadi схема.gvn | код.c [результат.png] "
              "[ещё.gvn ...] [-o результат.png] [--auto] [--scale=1.0] "
              "[--font=12] [--lw=1.1] [--dpi=200] [--show] [--gvn] "
              "[--template]", file=sys.stderr)
        return 2
    # вторая позиция вида результат.png — это выход, а не вход
    if (len(args) == 2 and output is None
            and re.search(r"\.(png|jpg|jpeg|svg)$", args[1], re.I)):
        output = args[1]
        args = args[:1]
    if len(args) > 1 and output:
        print("-o можно указывать только с одним файлом схемы", file=sys.stderr)
        return 2

    kw = dict(page=page, scale=scale, font=font, edge_lw=lw, dpi=dpi,
              c_labels=c_labels)
    if c_labels != "en":
        kw["c_labels"] = c_labels
    code = 0
    for inp in args:
        out = output if (output and len(args) == 1) else _default_output(inp)
        if inp.endswith(".c") and gvn:
            try:
                gvn_text = c_to_gvn(open(inp, encoding="utf-8").read())
                gpath = os.path.splitext(inp)[0] + ".gvn"
                with open(gpath, "w", encoding="utf-8") as f:
                    f.write(gvn_text)
                print(gpath)
            except ParseError as e:
                print(f"ошибка кода: {e}", file=sys.stderr)
                code = 1
        rc, files = render_file(inp, out, **kw)
        code = max(code, rc)
        if show and files:
            for f in files:
                if not _show_in_terminal(f):
                    print(f"терминал без графики — картинка в файле: {f}")
    return code


if __name__ == "__main__":
    sys.exit(main(sys.argv))
