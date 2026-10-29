"use strict";

/* ============================== состояние ============================== */

const EXAMPLE_C = `#include <stdio.h>

int main(void) {
    int month;

    printf("Enter month 1-12: ");
    if (scanf("%d", &month) != 1 || month < 1 || month > 12) {
        printf("Wrong input\\n");
        return 1;
    }

    switch (month / 3) {
        case 1: printf("Spring\\n"); break;
        case 2: printf("Summer\\n"); break;
        case 3: printf("Autumn\\n"); break;
        default: printf("Winter\\n");
    }

    return 0;
}`;

const $ = id => document.getElementById(id);

let wasm = null, engineReady = false;
let files = [];          // {name, stem, text, c, dirty}
let cur = -1;            // -1 — черновик без имени
let badNames = new Set();
let sheets = [];         // [{file, i, svg, w, h, name}]
let activeSheet = 0;
let freshDraw = false, lastSheetSig = "";
let zoom = null;         // null = вписать
let pan = { x: 0, y: 0 };
let renderTimer = null, pendingUndo = null, undoTimer = null, collageTimer = null, collageW = 0;
let lastBatch = false;

const store = {
  get(k) { try { return localStorage.getItem(k); } catch (e) { return null; } },
  set(k, v) { try { localStorage.setItem(k, v); } catch (e) {} },
  del(k) { try { localStorage.removeItem(k); } catch (e) {} },
};

/* ============================== тема ============================== */

/* Тема: auto (следует за системой) → light → dark по кругу. «Адатив» —
   дефолт по просьбе пользователя; ручной выбор запоминается. */
const THEME_CYCLE = { auto: "light", light: "dark", dark: "auto" };
let themeT = null;
function applyTheme(t) {
  document.documentElement.dataset.theme = t;
  store.set("gostpadi.theme", t);
  /* Класс живёт ровно один переход: иначе цвета щёлкают мгновенно. */
  document.documentElement.classList.add("theming");
  clearTimeout(themeT);
  themeT = setTimeout(() => document.documentElement.classList.remove("theming"), 260);
  const btn = $("theme");
  if (btn) {
    /* title снимаем: иначе перенос title→data-tip при старте
       затирает актуальную подпись старой из разметки. */
    btn.removeAttribute("title");
    btn.dataset.tip = { auto: "тема: как в системе", light: "тема: светлая", dark: "тема: тёмная" }[t];
  }
}
$("theme").addEventListener("click", () => {
  applyTheme(THEME_CYCLE[document.documentElement.dataset.theme] || "auto");
});
applyTheme(store.get("gostpadi.theme") || "auto");
addEventListener("resize", () => {
  if (zoom === null) applyZoom();
  // коллаж считает плитки от ширины панели: при повороте телефона
  // или сужении окна лист оставался прежним и уезжал за край
  const st = $("stage");
  if (st.classList.contains("collage")) {
    clearTimeout(collageTimer);
    collageTimer = setTimeout(() => {
      if (st.classList.contains("collage") && ownSheets().length && st.clientWidth !== collageW) drawSheets();
    }, 120);
  }
});

/* ============================== статус и тосты ============================== */

function status(html, isErr) {
  const el = $("statusline");
  el.className = isErr ? "err" : "";
  const m = $("msg");
  if (!m) return;
  m.innerHTML = html;
  m.classList.remove("flash");
  void m.offsetWidth;
  m.classList.add("flash");
}
let toastTimer = null;
function toast(html) {
  const el = $("toast");
  el.innerHTML = html;
  el.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { el.hidden = true; }, 5000);
  const b = el.querySelector("button");
  if (b) b.addEventListener("click", undoRemove, { once: true });
}

/* ============================== подсказки ============================== */

/* Нативный title ждёт секунду и рисуется системным шрифтом. Держим
   подсказки в data-tip и показываем свои: 350 мс, в тон интерфейсу. */
const tip = document.createElement("div");
tip.id = "tip"; tip.setAttribute("aria-hidden", "true");
document.body.appendChild(tip);
let tipTimer = null, tipEl = null;
function hideTip() {
  clearTimeout(tipTimer);
  tip.classList.remove("on");
  tipEl = null;
}
function showTip(el) {
  const text = el.dataset.tip;
  if (!text) return;
  tip.textContent = text;
  tip.classList.add("on");
  const r = el.getBoundingClientRect();
  const w = tip.offsetWidth, h = tip.offsetHeight;
  const x = Math.max(8, Math.min(r.left + r.width / 2 - w / 2, innerWidth - w - 8));
  const y = r.top - h - 8 < 8 ? r.bottom + 8 : r.top - h - 8;
  tip.style.left = x + "px";
  tip.style.top = y + "px";
}
function armTip(el) {
  if (!el || el === tipEl) return;
  hideTip();
  tipEl = el;
  tipTimer = setTimeout(() => showTip(el), 350);
}
document.addEventListener("pointerover", e => armTip(e.target.closest?.("[data-tip]")));
document.addEventListener("pointerout", e => { if (e.target.closest?.("[data-tip]")) hideTip(); });
/* Только клавиатурный фокус: после клика мышью подсказка не нужна. */
let lastPointer = 0;
document.addEventListener("pointerdown", () => { lastPointer = Date.now(); }, true);
document.addEventListener("focusin", e => {
  const el = e.target.closest?.("[data-tip]");
  /* Фокус после клика мышью — это не запрос подсказки: она всплывала
     поверх только что открытой панели. */
  if (el && el.matches(":focus-visible") && Date.now() - lastPointer > 600) armTip(el);
});
document.addEventListener("focusout", hideTip);
addEventListener("scroll", hideTip, true);
addEventListener("pointerdown", hideTip, true);

/* title → data-tip, включая элементы, которые JS создаёт позже. */
const toTip = el => {
  if (!el.title) return;
  el.dataset.tip = el.title;
  el.removeAttribute("title");
};
document.querySelectorAll("[title]").forEach(toTip);
new MutationObserver(list => {
  for (const m of list) {
    if (m.type === "attributes") toTip(m.target);
    else for (const n of m.addedNodes)
      if (n.nodeType === 1) { toTip(n); n.querySelectorAll("[title]").forEach(toTip); }
  }
}).observe(document.body, {
  subtree: true, childList: true, attributes: true, attributeFilter: ["title"],
});

/* ============================== wasm-движок ============================== */

async function loadEngine() {
  const r = await fetch("gostpadi.wasm");
  if (!r.ok) throw new Error("gostpadi.wasm: HTTP " + r.status);
  const { instance } = await WebAssembly.instantiate(await r.arrayBuffer(), {});
  wasm = instance.exports;
  engineReady = true;
  $("render").disabled = false;
  status("готово — схема появится сама при вводе");
  renderNow();
}

function wasmInput(bytes) {
  const p = wasm.gostpadi_alloc(bytes.length);
  if (p) new Uint8Array(wasm.memory.buffer, p, bytes.length).set(bytes);
  return [p, bytes.length];
}

function readOut(out, lp) {
  // view строим строго после вызова: память могла вырасти
  const n = new DataView(wasm.memory.buffer).getUint32(lp, true);
  if (!out || !n) return null;
  return {
    out, n,
    json: new TextDecoder().decode(new Uint8Array(wasm.memory.buffer, out, n)),
  };
}

function packInputs(items) {
  const enc = new TextEncoder();
  const parts = [new Uint8Array(new Uint32Array([items.length]).buffer)];
  for (const it of items) {
    const n = enc.encode(it.name), t = enc.encode(it.text);
    // протокол движка: [длина имени][имя][вид входа][длина текста][текст].
    // вид: 0 — .gvn (как есть), 1 — C (.c/.h), 2 — C++ (.cpp/.cc/.cxx/...).
    // Байта не было: движок читал первый байт длины текста как вид входа,
    // съезжал на 4 байта и отвечал null — «движок не ответил» на любом коде.
    const kind = /\.(cpp|cc|cxx|c\+\+|hpp|hxx|hh|h\+\+)$/i.test(it.name) ? 2
               : /\.(c|h)$/i.test(it.name) ? 1 : 0;
    parts.push(new Uint8Array(new Uint32Array([n.length]).buffer), n,
               new Uint8Array([kind]),
               new Uint8Array(new Uint32Array([t.length]).buffer), t);
  }
  // склейка: части идут подряд, без неё движок получает нули
  const all = new Uint8Array(parts.reduce((s, p) => s + p.length, 0));
  let o = 0;
  for (const p of parts) { all.set(p, o); o += p.length; }
  return all;
}

function callBatch(items, ru, lw, font) {
  const [ip, il] = wasmInput(packInputs(items));
  const lp = wasm.gostpadi_alloc(4);
  const r = readOut(wasm.gostpadi_render_batch(ip, il, ru ? 1 : 0, lw, font, lp), lp);
  wasm.gostpadi_free(ip, il);
  wasm.gostpadi_free(lp, 4);
  if (r) wasm.gostpadi_free(r.out, r.n);
  return JSON.parse(r ? r.json : '{"ok":false}');
}

/* Экспорт другим выходом движка: 1 — SVG по содержимому в
   миллиметрах, 2 — draw.io. Отдельный вызов, а не поле в render_batch:
   эти файлы нужны по кнопке, а на каждый чих перерисовки в JSON они
   бы только раздували ответ втрое. */
function callExport(items, ru, lw, font, mode) {
  if (!wasm || !wasm.gostpadi_export_batch) return null;
  const [ip, il] = wasmInput(packInputs(items));
  const lp = wasm.gostpadi_alloc(4);
  const r = readOut(wasm.gostpadi_export_batch(ip, il, ru ? 1 : 0, lw, font, mode, lp), lp);
  wasm.gostpadi_free(ip, il);
  wasm.gostpadi_free(lp, 4);
  if (r) wasm.gostpadi_free(r.out, r.n);
  return r ? JSON.parse(r.json) : null;
}

/* ============================== рендер ============================== */

function scheduleRender() {
  if (!engineReady) return;
  clearTimeout(renderTimer);
  renderTimer = setTimeout(renderNow, 500);
}

function syncCur() {
  if (cur >= 0 && files[cur]) files[cur].text = $("code").value;
}

function curItems() {
  if (cur < 0) {
    const text = $("code").value;
    return [{ name: "черновик.c", stem: "черновик", text }];
  }
  return files;
}

/* Убрать схему с экрана: после сбоя показывать прошлую схему —
   враньё, она сделана из другого кода. */
function dropSheets() {
  sheets = [];
  drawSheets();
  setExport(false);
  $("errbox").classList.remove("show");
}

function renderNow() {
  if (!engineReady) return;
  syncCur();
  const items = curItems().filter(it => it.text.trim());
  if (!items.length) {
    sheets = []; drawSheets();
    setExport(false);
    $("errbox").classList.remove("show");
    status("пусто — вставь код");
    return;
  }
  status('<span class="spinner"></span> рисую…');
  $("render").disabled = true;
  badNames = new Set();
  requestAnimationFrame(() => {
    const t0 = performance.now();
    const ru = $("ru").checked, lw = parseFloat($("lw").value),
          font = parseFloat($("font").value);
    let res;
    try {
      // всегда пакетом, даже для одного файла: пакетный путь рисует
      // ВСЕ функции файла, а одиночный только main. Два пути давали
      // разный результат на одном и том же коде.
      res = callBatch(items, ru, lw, font);
    } catch (e) {
      $("render").disabled = false;
      // старую схему сносим: под красной ошибкой оставалась схема
      // предыдущего успешного рендера, и выглядело так, будто
      // всё нарисовано из испорченного кода
      dropSheets();
      status("движок упал: " + e, true);
      return;
    }
    $("render").disabled = false;
    if (!res.ok) {
      // мост не падает на разборе: сюда попадает только внутренняя ошибка
      dropSheets();
      status("движок не ответил — попробуй ещё раз", true);
      return;
    }
    applyResult(res, items, performance.now() - t0);
  });
}

function applyResult(res, items, ms) {
  // Ошибок разбора больше нет: код без main рисуется пустым, такие
  // перечислены в "empty" и помечаются в списке файлов.
  // Пустой вход (пустой main, файл из одних return/объявлений) —
  // движок отдаёт имя, иногда со своим msg. Пустых схем не бывает:
  // такой вход — отказ с человеческим текстом («нет рисуемых
  // операторов»).
  const errors = [...(res.empty || []), ...(res.errors || [])].map(e => {
    const str = typeof e === "string";
    const msg = str ? "" : (e.msg || "");
    return {
      name: str ? e : e.name,
      line: !str && e.line != null ? e.line : null,
      col: !str && e.col != null ? e.col : null,
      src: !str ? e.src : null,
      msg: msg || "в коде нет рисуемых операторов — рисовать нечего",
      empty: str || msg.includes("нет рисуемых операторов"),
    };
  });
  badNames = new Set(errors.map(e => e.name));
  sheets = [];
  if (res.ok) {
    for (const f of res.files) {
      // движок отдаёт по схеме на функцию: «файл.c#функция». Листы
      // группируются по файлу, а подпись коллажа — имя функции.
      const hash = f.name.indexOf("#");
      const file = hash < 0 ? f.name : f.name.slice(0, hash);
      const fn = hash < 0 ? null : f.name.slice(hash + 1);
      f.sheets.forEach((svg, i) => {
        sheets.push({
          file, fn, i,
          svg,
          w: svgDims(svg)?.w || 800, h: svgDims(svg)?.h || 560,
          // имя для выгрузки: без расширения входа и с функцией
          name: stemOf(file) + (fn ? "-" + fn : "") + (f.sheets.length > 1 ? "-" + (i + 1) : ""),
        });
      });
    }
  }
  drawFileList();
  // «новый результат» — это смена набора схем, а не каждая перерисовка
  // того же кода: иначе коллаж мигал бы на каждом нажатии клавиши
  const sig = sheets.map(s => s.name + "#" + s.i).join("|");
  freshDraw = sheets.length > 0 && sig !== lastSheetSig;
  lastSheetSig = sig;
  // активный лист: лист активного файла; иначе первый
  let idx = sheets.findIndex(s => s.file === activeName());
  if (idx < 0) idx = 0;
  activeSheet = idx;
  drawSheets();
  setExport(sheets.length > 0);
  const errFiles = items.filter(it => badNames.has(it.name));
  showErrors(errors, items);
  const ms_ = ms >= 1000 ? (ms / 1000).toFixed(1) + " с" : Math.round(ms) + " мс";
  // схема — это файл+функция, а не лист: у файла с пятью функциями
  // листов десять, а схем пять
  const schemes = new Set(sheets.map(x => x.file + "#" + (x.fn || ""))).size;
  const pages = sheets.length;
  const batchNote = items.length > 1
    ? ' · <span class="batchnote">пачка: блоки одного размера</span>' : "";
  const what = schemes === pages
    ? `${pages} ${plural(pages, "лист", "листа", "листов")}`
    : `${schemes} ${plural(schemes, "схема", "схемы", "схем")}`;
  if (errors.length) {
    status(`ошибка: ${[...badNames].join(", ")}`, true);
    return;
  }
  status(`готово: ${what} · ${ms_}${batchNote}`);
  if (cur < 0) history.replaceState(null, "", "#c=" + b64urlEncode($("code").value));
  persist();
}

function plural(n, one, few, many) {
  if (n === 1) return one;
  if (n >= 2 && n <= 4) return few;
  return many;
}

function svgDims(svg) {
  const m = svg.match(/width="([\d.]+)(pt|mm)" height="([\d.]+)\2"/);
  if (!m) return null;
  const k = m[2] === "mm" ? 96 / 25.4 : 4 / 3;
  return { w: Math.round(+m[1] * k), h: Math.round(+m[3] * k) };
}

/* Габарит содержимого в координатах страницы — его движок кладёт в
   data-box. Плитка коллажа обрезается по нему: лист A4 с крошечной
   схемой посреди пустоты в коллаже выглядит пустым, и листать
   приходится вслепую. */
function contentBox(svg) {
  const m = svg.match(/data-box="([-\d. ]+)"/);
  if (!m) return null;
  const v = m[1].trim().split(/\s+/).map(Number);
  return v.length === 4 && v[2] > 0 && v[3] > 0 ? { x: v[0], y: v[1], w: v[2], h: v[3] } : null;
}

/* ============================== отрисовка листов ============================== */

/* Имя активного файла — ровно то, что движок вернул в `files[].name`.
   Раньше здесь стояло "scheme" для черновика, а движок отдаёт "черновик":
   список своих листов был пуст, и в коллаже не появлялось ничего. */
function activeName() {
  if (cur >= 0 && files[cur]) return files[cur].name;
  return curItems()[0]?.name || "схема";
}

/* Листы активного файла: в коллаже и в просмотре показываем одни и
   те же, иначе стрелка «дальше» уводит на лист другого файла. */
function ownSheets() {
  const name = activeName();
  return sheets.filter(s => s.file === name);
}

/* Плитка одного листа: карточка фиксированного размера + подпись.
   Размер задаёт коллаж, а не лист: превью должны быть одного
   масштаба между собой, иначе сравнивать схемы нечем. */
function tileFor(s, w, h, cap, crop) {
  const tile = document.createElement("button");
  tile.type = "button";
  tile.className = "tile";
  const card = document.createElement("div");
  card.className = "sheet";
  card.style.width = w + "px";
  card.style.height = h + "px";
  const holder = document.createElement("div");
  holder.innerHTML = s.svg;
  const svg = holder.firstElementChild;
  svg.style.width = "100%"; svg.style.height = "100%";
  // обрезка листа по содержимому: viewBox в координатах страницы,
  // блок внутри листа остаётся на своём месте
  const box = crop && contentBox(s.svg);
  if (box) {
    svg.setAttribute("viewBox", `${box.x} ${box.y} ${box.w} ${box.h}`);
    svg.removeAttribute("width");
    svg.removeAttribute("height");
  }
  card.appendChild(svg);
  const label = document.createElement("div");
  label.className = "cap";
  label.textContent = cap;
  tile.append(card, label);
  tile.title = s.name + " — открыть";
  return tile;
}

/* Коллаж: все листы файла сразу мелкими плитками. Раньше лист был
   один, а остальные прятались за вкладками с цифрами — вкладки не
   говорят, сколько их, и где ты сейчас. */
let suppressClick = false;
function drawCollage(stage) {
  const own = ownSheets();
  stage.className = "collage";
  if (freshDraw) {
    stage.classList.add("fresh");
    setTimeout(() => stage.classList.remove("fresh"), 200);
    freshDraw = false;
  }
  collageW = stage.clientWidth;
  const vh = stage.clientHeight - 60 || 320;
  // коллаж читается коллажем, когда плиток несколько. Одна схема —
  // не повод показывать её маркой в углу: даём ей место панели.
  const solo = own.length === 1;
  const maxH = solo
    ? Math.max(180, Math.min(vh - 40, 520))
    : Math.max(110, Math.min(230, vh / 2));
  // лента горизонтальная, поэтому плитка ограничена шириной панели
  const maxW = solo
    ? Math.max(180, Math.min(stage.clientWidth - 32, 640))
    : Math.max(110, Math.min(240, (stage.clientWidth || 480) / 2 - 20));
  const manyFiles = new Set(own.map(s => s.file)).size > 1;
  own.forEach((s, i) => {
    // пропорции берём у содержимого, а не у листа: иначе плитка A4
    // с пятистрочной схемой посреди пустоты
    const box = contentBox(s.svg);
    const ar = box ? box.w / box.h : s.w / s.h;
    // Обе границы, а не только высота: у схемы из трёх блоков габарит
    // низкий и широкий, и по одной высоте плитка выходила в 1400 px.
    let w = Math.round(maxH * ar);
    let h = maxH;
    if (w > maxW) { w = maxW; h = Math.max(90, Math.round(w / ar)); }
    // подпись: функция важнее номера листа. Без неё десяток схем
    // одного файла выглядит как десяток безымянных картинок.
    let cap = s.fn || s.file;
    // номер листа — только если у ЭТОЙ функции их несколько: у main
    // один лист, а «лист 1» рядом с именем функции был бы шумом
    const sameFn = own.filter(x => x.fn === s.fn);
    if (sameFn.length > 1) cap += " · лист " + (sameFn.indexOf(s) + 1);
    if (manyFiles) cap += " · " + s.file;
    const tile = tileFor(s, w, h, cap, true);
    tile.addEventListener("click", () => { if (!suppressClick) openLightbox(i); });
    stage.appendChild(tile);
  });
  suppressClick = false;
  depthPass(stage);
  wireCollageScroll(stage);
}

/* Параллакс: плитка на расстоянии d плиток от центра ленты бледнеет.
   Расстояние считаем по прокрутке, а не по индексу, — иначе при
   прокрутке вбок глубина не менялась бы и эффект стоял на месте. */
function depthPass(stage) {
  const tiles = [...stage.children];
  if (tiles.length < 3) return;
  const paint = () => {
    const mid = stage.scrollLeft + stage.clientWidth / 2;
    const step = tiles[1].offsetLeft - tiles[0].offsetLeft || 1;
    for (const t of tiles) {
      const d = Math.abs(t.offsetLeft + t.offsetWidth / 2 - mid) / step;
      t.style.setProperty("--depth", Math.max(0.42, 1 - d * 0.16).toFixed(3));
    }
  };
  paint();
  stage.addEventListener("scroll", paint, { passive: true });
  // Панель может быть скрыта в момент отрисовки — на узком экране
  // видна только активная вкладка, ширина нулевая, и все плитки
  // получают глубину 1. Пересчитываем, когда лента обретает размер.
  new ResizeObserver(paint).observe(stage);
}

/* Лента листается: колесо горизонтально, перетаскивание мышью и
   пальцем. Без этого половина схем в лабе не видна — их десяток, а
   панель узкая. */
function wireCollageScroll(stage) {
  stage.addEventListener("wheel", e => {
    if (Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return;
    // вертикальное колесо листает ленту, а не страницу под ней
    if (stage.scrollWidth <= stage.clientWidth + 1) return;
    e.preventDefault();
    stage.scrollLeft += e.deltaY;
  }, { passive: false });
  let drag = null;
  stage.addEventListener("pointerdown", e => {
    suppressClick = false;
    if (stage.scrollWidth <= stage.clientWidth + 1) return;
    drag = { x: e.clientX, left: stage.scrollLeft, moved: 0 };
  });
  stage.addEventListener("pointermove", e => {
    if (!drag) return;
    const d = e.clientX - drag.x;
    drag.moved = Math.max(drag.moved, Math.abs(d));
    stage.scrollLeft = drag.left - d;
  });
  const stop = () => {
    // клик после перетаскивания — не клик по плитке
    if (drag && drag.moved > 6) suppressClick = true;
    drag = null;
  };
  stage.addEventListener("pointerup", stop);
  stage.addEventListener("pointercancel", stop);
}

function drawSheets() {
  const stage = $("stage");
  stage.innerHTML = "";
  const ph = $("placeholder");
  if (!sheets.length) {
    ph.hidden = false;
    stage.className = "";
    return;
  }
  ph.hidden = true;
  // один режим: коллаж плиток, крупный просмотр — лайтбокс по клику
  drawCollage(stage);
}

/* ============================== полный просмотр ============================== */

/* В панели схема всегда мелкая: колонка узкая, и на листе А4 блоки
   не прочесть. Полный просмотр — лист во весь экран, с масштабом,
   листанием и закрытием по Esc. Открывается кликом по плитке в
   коллаже или кнопкой на схеме в режиме просмотра. */
let lbZoom = null, lbPan = { x: 0, y: 0 }, lbOpen = false;

function lbSheet() {
  return sheets[Math.min(activeSheet, sheets.length - 1)] || sheets[0];
}

function openLightbox(idx) {
  if (idx !== undefined && idx !== null) {
    const own = ownSheets();
    if (!own.length) return;
    activeSheet = sheets.indexOf(own[Math.max(0, Math.min(idx, own.length - 1))]);
  }
  if (!lbSheet()) return;
  lbOpen = true;
  lbZoom = null;
  $("lightbox").hidden = false;
  $("zoomcard").hidden = true;
  lbDraw();
  $("lbclose").focus();
}

/* Плитка коллажа ведёт в полный просмотр, а не в узкую панель: клик
   по схеме должен показывать схему. */
function closeLightbox() {
  lbOpen = false;
  $("lightbox").hidden = true;
  $("zoomcard").hidden = sheets.length === 0;
}

function lbSize() {
  const v = $("lbview");
  return { w: v.clientWidth, h: v.clientHeight };
}

function lbFit() {
  const { w, h } = lbSize();
  const s = lbSheet();
  if (!s || w <= 0 || h <= 0) return null;
  return Math.max(0.05, Math.min(1.6, Math.min(w / s.w, h / s.h)));
}

function lbApply() {
  const stage = $("lbstage");
  const s = lbSheet();
  if (!s) return;
  if (!isFinite(lbZoom) || lbZoom <= 0) lbZoom = lbFit() ?? 1;
  const z = lbZoom;
  const { w, h } = lbSize();
  // центр вмещающейся оси — честный центр вьюпорта; выходящей за край —
  // свободный сдвиг с упором в 12px от кромки: лист нельзя потерять
  const cw = s.w * z, ch = s.h * z;
  const clampAxis = (size, view, need) =>
    need <= view ? (view - need) / 2 : Math.min(0, Math.max(view - need, size));
  if (lbPan === null) lbPan = { x: 0, y: 0 };
  lbPan.x = clampAxis(lbPan.x, w, cw);
  lbPan.y = clampAxis(lbPan.y, h, ch);
  stage.style.transform = `translate(${lbPan.x}px, ${lbPan.y}px) scale(${z})`;
  $("lbval").textContent = Math.round(z * 100) + "%";
}

function lbDraw() {
  const stage = $("lbstage");
  stage.innerHTML = "";
  const s = lbSheet();
  if (!s) { closeLightbox(); return; }
  const card = document.createElement("div");
  card.className = "sheet";
  card.style.width = s.w + "px";
  card.style.height = s.h + "px";
  const holder = document.createElement("div");
  holder.innerHTML = s.svg;
  const svg = holder.firstElementChild;
  svg.style.width = "100%"; svg.style.height = "100%";
  card.appendChild(svg);
  stage.appendChild(card);
  const own = ownSheets();
  $("lbtitle").textContent = (s.fn || s.file) + (s.file !== (s.fn || s.file) ? " · " + s.file : "");
  const multi = own.length > 1;
  $("lbpager").hidden = !multi;
  if (multi) {
    const at = own.indexOf(s);
    $("lbprev").disabled = at <= 0;
    $("lbnext").disabled = at >= own.length - 1;
    // плёнка миниатюр вместо точек: видно, какой лист какой, прыжок в один клик
    const thumbs = $("lbthumbs");
    thumbs.innerHTML = "";
    own.forEach((x, i) => {
      const d = document.createElement("div");
      d.className = "lbthumb";
      d.style.width = Math.round(Math.max(26, Math.min(76, 46 * (x.w / x.h)))) + "px";
      d.title = x.name;
      d.setAttribute("role", "tab");
      d.setAttribute("aria-current", i === at ? "true" : "false");
      d.innerHTML = x.svg;
      d.addEventListener("click", () => {
        const own = ownSheets();
        activeSheet = sheets.indexOf(own[i]);
        lbDraw();
      });
      thumbs.appendChild(d);
    });
    thumbs.children[at]?.scrollIntoView({ block: "nearest", inline: "center" });
  }
  lbZoom = null;
  lbPan = null;
  lbApply();
}

function lbStep(d) {
  const own = ownSheets();
  if (own.length < 2) return;
  const at = own.indexOf(lbSheet());
  const next = own[(at + d + own.length) % own.length];
  activeSheet = sheets.indexOf(next);
  lbDraw();
}

$("lbclose").addEventListener("click", closeLightbox);
$("lbout").addEventListener("click", () => { lbZoom = (lbZoom ?? lbFit() ?? 1) / 1.25; lbPan = null; lbApply(); });
$("lbin").addEventListener("click", () => { lbZoom = (lbZoom ?? lbFit() ?? 1) * 1.25; lbPan = null; lbApply(); });
$("lbfit").addEventListener("click", () => { lbZoom = null; lbPan = null; lbApply(); });
$("lbprev").addEventListener("click", () => lbStep(-1));
$("lbnext").addEventListener("click", () => lbStep(1));
$("zoomcard").addEventListener("click", () => openLightbox());
addEventListener("resize", () => { if (lbOpen) { lbZoom = null; lbPan = null; lbApply(); } });

/* Жесты лайтбокса: колесо — масштаб вокруг курсора, палец/мышь — сдвиг,
   щипок — масштаб вокруг середины пальцев, свайп на «вмещается» —
   соседний лист, двойной клик — «вместить ↔ крупно». Во время жеста
   анимация гасится (класс .lb-live), иначе переход плавает за пальцем. */
let lbLiveT = 0;
function lbLive() {
  const st = $("lbstage");
  st.classList.add("lb-live");
  clearTimeout(lbLiveT);
  lbLiveT = setTimeout(() => st.classList.remove("lb-live"), 260);
}

$("lbview").addEventListener("wheel", e => {
  if (!lbOpen) return;
  e.preventDefault();
  lbLive();
  const z0 = lbZoom ?? lbFit() ?? 1;
  const z = Math.max(0.05, Math.min(6, z0 * (e.deltaY < 0 ? 1.12 : 1 / 1.12)));
  const k = z / z0;
  if (!lbPan) lbApply();
  const v = $("lbview").getBoundingClientRect();
  // точка под курсором остаётся под курсором: pan' = m − (m − pan)·k
  const mx = e.clientX - v.left, my = e.clientY - v.top;
  lbPan = { x: mx - (mx - lbPan.x) * k, y: my - (my - lbPan.y) * k };
  lbZoom = z;
  lbApply();
}, { passive: false });

let lbPts = new Map(), lbDrag = null, lbPinch = null, lbLastMove = 0;
$("lbview").addEventListener("pointerdown", e => {
  if (!lbOpen || e.target.closest("button")) return;
  try { $("lbview").setPointerCapture(e.pointerId); } catch { /* указатель уже отжат */ }
  lbPts.set(e.pointerId, { x: e.clientX, y: e.clientY });
  if (lbPts.size === 1) {
    lbLive();
    lbDrag = { x: e.clientX, y: e.clientY, px: lbPan.x, py: lbPan.y, moved: 0 };
  } else if (lbPts.size === 2) {
    lbDrag = null;
    lbLive();
    if (!lbPan) lbApply();
    const [a, b] = [...lbPts.values()];
    lbPinch = {
      d: Math.hypot(a.x - b.x, a.y - b.y) || 1,
      mx: (a.x + b.x) / 2, my: (a.y + b.y) / 2,
      z: lbZoom, pan: { ...lbPan },
    };
  }
});
$("lbview").addEventListener("pointermove", e => {
  const p = lbPts.get(e.pointerId);
  if (p) { p.x = e.clientX; p.y = e.clientY; }
  if (lbPinch && lbPts.size >= 2) {
    lbLive();
    const [a, b] = [...lbPts.values()];
    const d = Math.hypot(a.x - b.x, a.y - b.y) || 1;
    const k = Math.max(0.05 / lbPinch.z, Math.min(6 / lbPinch.z, d / lbPinch.d));
    lbZoom = lbPinch.z * k;
    // середина щипка остаётся под пальцами
    const mx = (a.x + b.x) / 2, my = (a.y + b.y) / 2;
    lbPan = { x: mx - (lbPinch.mx - lbPinch.pan.x) * k, y: my - (lbPinch.my - lbPinch.pan.y) * k };
    lbApply();
  } else if (lbDrag && p) {
    const dx = e.clientX - lbDrag.x, dy = e.clientY - lbDrag.y;
    lbDrag.moved = Math.max(lbDrag.moved, Math.abs(dx) + Math.abs(dy));
    lbLive();
    lbPan = { x: lbDrag.px + dx, y: lbDrag.py + dy };
    lbApply();
  }
});
const lbDragEnd = e => {
  lbPts.delete(e.pointerId);
  if (lbPts.size === 1) {
    // из щипка в драг одним пальцем: якорь — оставшийся палец
    lbPinch = null;
    const [p] = [...lbPts.values()];
    lbDrag = { x: p.x, y: p.y, px: lbPan.x, py: lbPan.y, moved: Infinity };
    return;
  }
  if (lbDrag && !lbPts.size) {
    lbLastMove = lbDrag.moved;
    const dx = e.clientX - lbDrag.x, dy = e.clientY - lbDrag.y;
    const fit = lbFit() ?? 1;
    // свайп работает только на «вмещается»; щипок (moved: Infinity)
    // свайпом не считается. Влево — следующий лист.
    if (lbDrag.moved !== Infinity
      && Math.abs(dx) > 60 && Math.abs(dx) > 2 * Math.abs(dy)
      && (lbZoom ?? fit) <= fit * 1.02) {
      lbStep(dx < 0 ? 1 : -1);
    }
  }
  lbPinch = null;
  lbDrag = null;
};
$("lbview").addEventListener("pointerup", lbDragEnd);
$("lbview").addEventListener("pointercancel", lbDragEnd);

// двойной клик: «вместить ↔ крупно», якорь — точка под курсором
$("lbview").addEventListener("dblclick", e => {
  if (!lbOpen || lbLastMove > 8) return;
  const fit = lbFit() ?? 1;
  if ((lbZoom ?? fit) > fit * 1.01) { lbZoom = null; lbPan = null; lbApply(); return; }
  lbLive();
  const z0 = lbZoom ?? fit, z = Math.min(6, fit * 2), k = z / z0;
  if (!lbPan) lbApply();
  const v = $("lbview").getBoundingClientRect();
  const mx = e.clientX - v.left, my = e.clientY - v.top;
  lbPan = { x: mx - (mx - lbPan.x) * k, y: my - (my - lbPan.y) * k };
  lbZoom = z;
  lbApply();
});


function viewportSize() {
  const v = $("viewport");
  return { w: v.clientWidth - 28, h: v.clientHeight - 28 };
}

// Панель может быть скрыта (data-mtab), и тогда clientWidth = 0, то есть
// вьюпорт «-28 на -28». Деление на нулевой размер карточки давало
// -Infinity, scale(-Infinity) браузер отвергал — трансформация оставалась
// пустой и схема уезжала за край, а подпись показывала «-Infinity%».
// Нет разметки — нет и масштаба.
function fitZoom() {
  const { w, h } = viewportSize();
  if (w <= 0 || h <= 0) return null;
  const stage = $("stage");
  if (stage.classList.contains("collage")) return null;
  const cards = stage.children;
  if (!cards.length) return null;
  let maxX = 0, maxY = 0;
  for (const c of cards) {
    maxX = Math.max(maxX, c.offsetLeft + c.offsetWidth);
    maxY = Math.max(maxY, c.offsetTop + c.offsetHeight);
  }
  if (maxX <= 0 || maxY <= 0) return null;
  return Math.min(1.6, Math.min(w / maxX, h / maxY));
}

function applyZoom() {
  const stage = $("stage");
  // коллаж не трансформируется: плитки и так вписаны, а трансформация
  // увеличила бы их относительно точки отсчёта и порвала раскладку
  if (stage.classList.contains("collage")) return;
  let z = zoom, p = pan;
  // мусорный зафиксированный масштаб (было -Infinity) возвращаем
  // к вписыванию, а не рисуем схему за краем экрана
  if (!isFinite(zoom) || zoom <= 0) zoom = null;
  if (zoom === null) {
    z = fitZoom() ?? 1;
    // центр: content вписан и отцентрован в вьюпорте
    let maxX = 0, maxY = 0;
    for (const c of stage.children) {
      maxX = Math.max(maxX, c.offsetLeft + c.offsetWidth);
      maxY = Math.max(maxY, c.offsetTop + c.offsetHeight);
    }
    const { w, h } = viewportSize();
    p = {
      x: Math.max(6, (w - maxX * z) / 2 + 6),
      y: Math.max(6, (h - maxY * z) / 2 + 6),
    };
  }
  stage.style.transform = `translate(${p.x}px, ${p.y}px) scale(${z})`;
  $("zoomval").textContent = Math.round(z * 100) + "%";
}

function setZoom(z) {
  zoom = z;
  if (zoom === null) pan = { x: 10, y: 10 };
  applyZoom();
}

$("zoomin").addEventListener("click", () => setZoom(curZoom() * 1.25));
$("zoomout").addEventListener("click", () => setZoom(curZoom() / 1.25));
$("zoomfit").addEventListener("click", () => setZoom(null));

function curZoom() {
  return zoom ?? fitZoom() ?? 1;
}

// колесо — зум к курсору; тянуть — панорама
(() => {
  const v = $("viewport");
  v.addEventListener("wheel", e => {
    // коллаж листается сам (см. wireCollageScroll), а масштаб у него
    // всё равно отключён CSS: transform: none !important. Без этой
    // проверки колесо над коллажем уводило бы в зум, которого не видно
    if (e.target.closest("#stage.collage")) return;
    e.preventDefault();
    const rect = v.getBoundingClientRect();
    const cx = e.clientX - rect.left, cy = e.clientY - rect.top;
    const old = curZoom();
    const factor = e.deltaY < 0 ? 1.12 : 1 / 1.12;
    const nz = Math.min(4, Math.max(0.05, old * factor));
    if (zoom === null) {
      // фиксировали текущий вписанный масштаб
      zoom = old;
      pan = { x: 10, y: 10 };
    }
    pan.x = cx - (cx - pan.x) * (nz / old);
    pan.y = cy - (cy - pan.y) * (nz / old);
    zoom = nz;
    applyZoom();
  }, { passive: false });
  let drag = null;
  v.addEventListener("pointerdown", e => {
    // Плитка коллажа — кнопка: нажатие по ней должно открывать полный
    // просмотр, а не начинать панораму. setPointerCapture перенаправляет
    // последующие события указателя (и click) на вьюпорт, и плитка их
    // не получала вовсе — схема не открывалась. Захват берём только
    // когда тянуть действительно есть что: по фону или по схеме в
    // режиме просмотра.
    if (e.target.closest("button")) return;
    drag = { x: e.clientX, y: e.clientY, px: pan.x, py: pan.y };
    v.classList.add("panning");
    v.setPointerCapture(e.pointerId);
  });
  v.addEventListener("pointermove", e => {
    if (!drag) return;
    pan.x = drag.px + (e.clientX - drag.x);
    pan.y = drag.py + (e.clientY - drag.y);
    if (zoom === null) zoom = fitZoom() ?? 1;
    applyZoom();
  });
  v.addEventListener("pointerup", () => { drag = null; v.classList.remove("panning"); });
  v.addEventListener("pointercancel", () => { drag = null; v.classList.remove("panning"); });
})();

/* ============================== экспорт ============================== */

function setExport(on) {
  $("dx-open").disabled = !on;
  $("copy").disabled = !on;
}

/* Принимает и URL, и Blob. Раньше принимала только URL, и кнопка
   draw.io отдавала Blob — в href попадал «[object Blob]», и файл не
   сохранялся вовсе. */
function save(what, name) {
  const a = document.createElement("a");
  const url = typeof what === "string" ? what : URL.createObjectURL(what);
  a.href = url;
  a.download = name;
  a.click();
  // освобождать сразу нельзя: браузер успевает начать только после
  // возврата в цикл событий, и отзыв URL обрывает загрузку
  if (typeof what !== "string") setTimeout(() => URL.revokeObjectURL(url), 10000);
}

/* Имя файла на экспорт: то же, что у листа на экране, но без
   номера страницы — по содержимому схема одна, не по листам. */
function exportStem(s) {
  return s ? s.name.replace(/-\d+$/, "") : "схема";
}

/* Ответ движка — по схеме на функцию, а кнопка одна. Брать files[0]
   нельзя: открыт может быть не main, и молча скачался бы другой файл.
   Ищем по имени схемы, а если не нашлось — берём первую (и это
   единственный случай, когда ответ может быть не тем). */
function pickExport(json, sheet) {
  if (!json || !json.ok || !json.files || !json.files.length) return null;
  if (sheet) {
    const want = sheet.fn ? sheet.file + "#" + sheet.fn : sheet.file;
    const hit = json.files.find(f => f.name === want);
    if (hit) return hit;
  }
  return json.files[0];
}

/* ---------- ZIP без библиотек: store, UTF-8 имена, CRC32 ---------- */

const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let i = 0; i < 256; i++) {
    let c = i;
    for (let k = 0; k < 8; k++) c = (c & 1) ? (0xEDB88320 ^ (c >>> 1)) : (c >>> 1);
    t[i] = c >>> 0;
  }
  return t;
})();

function crc32(buf) {
  let c = 0xFFFFFFFF;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xFF] ^ (c >>> 8);
  return (c ^ 0xFFFFFFFF) >>> 0;
}

function zipStore(files) {
  const enc = new TextEncoder();
  const d = new Date();
  const time = (d.getHours() << 11) | (d.getMinutes() << 5) | (d.getSeconds() >> 1);
  const date = ((d.getFullYear() - 1980) << 9) | ((d.getMonth() + 1) << 5) | d.getDate();
  const chunks = [], central = [];
  let offset = 0;
  for (const f of files) {
    const name = enc.encode(f.name);
    const data = f.data;
    const sum = crc32(data);
    const lh = new Uint8Array(30 + name.length);
    const lv = new DataView(lh.buffer);
    lv.setUint32(0, 0x04034b50, true);
    lv.setUint16(4, 20, true);
    lv.setUint16(6, 0x0800, true); // имена в UTF-8
    lv.setUint16(8, 0, true);      // store: без сжатия
    lv.setUint16(10, time, true);
    lv.setUint16(12, date, true);
    lv.setUint32(14, sum, true);
    lv.setUint32(18, data.length, true);
    lv.setUint32(22, data.length, true);
    lv.setUint16(26, name.length, true);
    lh.set(name, 30);
    chunks.push(lh, data);

    const ch = new Uint8Array(46 + name.length);
    const cv = new DataView(ch.buffer);
    cv.setUint32(0, 0x02014b50, true);
    cv.setUint16(4, 20, true);
    cv.setUint16(6, 20, true);
    cv.setUint16(8, 0x0800, true);
    cv.setUint16(12, time, true);
    cv.setUint16(14, date, true);
    cv.setUint32(16, sum, true);
    cv.setUint32(20, data.length, true);
    cv.setUint32(24, data.length, true);
    cv.setUint16(28, name.length, true);
    cv.setUint32(42, offset, true);
    ch.set(name, 46);
    central.push(ch);
    offset += lh.length + data.length;
  }
  const cdSize = central.reduce((n, c) => n + c.length, 0);
  const end = new Uint8Array(22);
  const ev = new DataView(end.buffer);
  ev.setUint32(0, 0x06054b50, true);
  ev.setUint16(8, files.length, true);
  ev.setUint16(10, files.length, true);
  ev.setUint32(12, cdSize, true);
  ev.setUint32(16, offset, true);
  return new Blob([...chunks, ...central, end], { type: "application/zip" });
}

/* ---------- PNG 300 dpi: размер в пунктах × 300/72 ---------- */

function svgPtSize(svgText) {
  const m = svgText.match(/<svg[^>]*\bwidth="([\d.]+)(pt|mm)"[^>]*\bheight="([\d.]+)\2"/);
  if (!m) return null;
  const k = m[2] === "mm" ? 72 / 25.4 : 1;
  return { w: +m[1] * k, h: +m[3] * k };
}

function svgToPng(svgText) {
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(new Blob([svgText], { type: "image/svg+xml;charset=utf-8" }));
    const img = new Image();
    img.onload = () => {
      try {
        const pt = svgPtSize(svgText) || { w: img.width * 72 / 96, h: img.height * 72 / 96 };
        const k = 300 / 72;
        const c = document.createElement("canvas");
        c.width = Math.max(1, Math.round(pt.w * k));
        c.height = Math.max(1, Math.round(pt.h * k));
        const ctx = c.getContext("2d");
        ctx.fillStyle = "#fff";
        ctx.fillRect(0, 0, c.width, c.height);
        ctx.drawImage(img, 0, 0, c.width, c.height);
        c.toBlob(b => {
          URL.revokeObjectURL(url);
          if (b && b.size) resolve(b);
          else reject(new Error("PNG получился пустым"));
        }, "image/png");
      } catch (e) {
        URL.revokeObjectURL(url);
        reject(e);
      }
    };
    img.onerror = () => {
      URL.revokeObjectURL(url);
      reject(new Error("SVG листа не открылся"));
    };
    img.src = url;
  });
}

/* ---------- попап скачивания ---------- */

let dxFmt = "svg", dxScope = "all", dxWorking = false, dxLastFocus = null;

function dxCount(n, one, few, many) { return n + " " + plural(n, one, few, many); }

function dxSheets() {
  if (dxScope === "all") return sheets.slice();
  const s = sheets[activeSheet] || sheets[0];
  return s ? [s] : [];
}

function dxUpdate() {
  if (!sheets.length) return;
  const schemes = new Set(sheets.map(s => s.file + "#" + (s.fn || ""))).size;
  $("dx-lead").innerHTML = `<b>${dxCount(schemes, "схема", "схемы", "схем")}</b> · ` +
    `${dxCount(sheets.length, "лист", "листа", "листов")}. Выбери формат — файл собирается в браузере.`;
  const list = dxSheets();
  const s = sheets[activeSheet] || sheets[0];
  let sum;
  if (dxFmt === "drawio") {
    sum = dxScope === "all"
      ? "1 файл · все схемы в одной книге"
      : "1 файл · " + exportStem(s) + ".drawio";
  } else if (list.length > 1) {
    sum = `${dxCount(list.length, "файл", "файла", "файлов")} · ZIP`;
  } else {
    sum = `1 файл · ${list[0].name}.${dxFmt === "png" ? "png" : "svg"}`;
  }
  $("dx-sum").textContent = sum;
}

function dxMsg(text, bad) {
  const el = $("dx-msg");
  el.textContent = text || "";
  el.classList.toggle("err", !!bad);
}

function dxShowForm() {
  $("dx-form").hidden = false;
  $("dx-scope").hidden = false;
  $("dx-lead").hidden = false;
  $("dx-done").hidden = true;
  $("dx-go").hidden = false;
  $("dx-again").hidden = true;
  $("dx-finish").hidden = true;
  $("dx-help-p").hidden = true;
  $("dx-help").hidden = false;
  $("dx-help").setAttribute("aria-expanded", "false");
}

function dxOpen() {
  if (!sheets.length) return;
  dxLastFocus = document.activeElement;
  dxScope = "all";
  dxFmt = "svg";
  $("dx-all").classList.add("on");
  $("dx-one").classList.remove("on");
  $("dx-all").setAttribute("aria-pressed", "true");
  $("dx-one").setAttribute("aria-pressed", "false");
  for (const c of document.querySelectorAll(".dx-card")) {
    const on = c.dataset.fmt === dxFmt;
    c.classList.toggle("on", on);
    c.querySelector("input").checked = on;
  }
  dxWorking = false;
  $("dx-go").disabled = false;
  $("dx-go").textContent = "Скачать";
  dxMsg("");
  dxShowForm();
  $("dx").hidden = false;
  dxUpdate();
  $("dx-close").focus();
}

function dxClose() {
  if ($("dx").hidden) return;
  $("dx").hidden = true;
  dxWorking = false;
  if (dxLastFocus && typeof dxLastFocus.focus === "function") dxLastFocus.focus();
}

const DX_STEPS = {
  svg: [
    "<b>Вставь в Word</b> — Вставка → Рисунок → этот .svg.",
    "<b>Печать</b> — Ctrl+P: линии останутся резкими в любом масштабе.",
    "<b>Правки</b> — меняй код и скачивай заново, схема та же.",
  ],
  png: [
    "<b>Открой</b> — файл покажет любой просмотрщик картинок.",
    "<b>Вставь</b> — в Word, PowerPoint, чат или презентацию.",
    "<b>Печать</b> — 300 dpi достаточно для листа А4.",
  ],
  drawio: [
    "<b>Открой</b> — app.diagrams.net или приложение draw.io.",
    "<b>Файл → Открыть</b> — выбери скачанный .drawio.",
    "<b>Двигай</b> — блоки и стрелки остаются редактируемыми.",
  ],
};

function dxDone(fileName, count, fmt, zipped) {
  $("dx-form").hidden = true;
  $("dx-scope").hidden = true;
  $("dx-lead").hidden = true;
  $("dx-help-p").hidden = true;
  $("dx-help").hidden = true;
  $("dx-done").hidden = false;
  $("dx-go").hidden = true;
  $("dx-again").hidden = false;
  $("dx-finish").hidden = false;
  $("dx-done-h").textContent = "Готово";
  $("dx-done-sub").textContent = zipped
    ? `${fileName} · ${dxCount(count, "файл", "файла", "файлов")} внутри`
    : fileName;
  // один span на шаг: иначе каждый текстовый узел становится отдельной
  // грид-ячейкой и шаг рассыпается на строки по слову
  $("dx-steps").innerHTML = DX_STEPS[fmt].map(s => `<li><span>${s}</span></li>`).join("");
  $("dx-hint").innerHTML = "Файл в папке загрузок. " +
    "<kbd>Ctrl</kbd>+<kbd>J</kbd> — список загрузок в браузере.";
  $("dx-finish").focus();
}

async function dxRun() {
  if (dxWorking || !sheets.length) return;
  dxWorking = true;
  const fmt = dxFmt, all = dxScope === "all";
  const list = dxSheets();
  const btn = $("dx-go");
  btn.disabled = true;
  btn.textContent = "Готовлю…";
  dxMsg("");
  try {
    const items = curItems().filter(it => it.text.trim());
    if (fmt === "drawio") {
      const r = callExport(items, $("ru").checked, +$("lw").value, +$("font").value, all ? 3 : 2);
      if (!r) throw new Error("движок не отдал файл — обнови страницу");
      if (!r.ok) throw new Error((r.error && r.error.msg) || "движок не смог собрать draw.io");
      const f = all ? r.files[0] : pickExport(r, list[0]);
      if (!f || !f.sheets || !f.sheets.length) throw new Error("в ответе движка нет книги draw.io");
      const base = all ? "схемы" : exportStem(list[0]);
      const name = base.replace(/\.drawio$/i, "") + ".drawio";
      save(new Blob([f.sheets[0]], { type: "application/xml" }), name);
      dxDone(name, 1, "drawio", false);
      return;
    }
    if (fmt === "svg") {
      const files = list.map(s => ({
        name: s.name + ".svg",
        data: new TextEncoder().encode(s.svg),
      }));
      if (files.length === 1) {
        save(new Blob([files[0].data], { type: "image/svg+xml;charset=utf-8" }), files[0].name);
        dxDone(files[0].name, 1, "svg", false);
      } else {
        const name = "gostpadi-svg.zip";
        save(zipStore(files), name);
        dxDone(name, files.length, "svg", true);
      }
      return;
    }
    // PNG: по одному, чтобы прогресс был виден и вкладка не замирала
    const files = [];
    for (let i = 0; i < list.length; i++) {
      btn.textContent = `PNG ${i + 1}/${list.length}…`;
      await new Promise(requestAnimationFrame);
      const blob = await svgToPng(list[i].svg);
      files.push({ name: list[i].name + ".png", data: new Uint8Array(await blob.arrayBuffer()) });
    }
    if (files.length === 1) {
      save(new Blob([files[0].data], { type: "image/png" }), files[0].name);
      dxDone(files[0].name, 1, "png", false);
    } else {
      const name = "gostpadi-png.zip";
      save(zipStore(files), name);
      dxDone(name, files.length, "png", true);
    }
  } catch (e) {
    dxMsg((e && e.message) || "не получилось скачать — попробуй ещё раз", true);
  } finally {
    dxWorking = false;
    btn.disabled = false;
    btn.textContent = "Скачать";
  }
}

$("dx-open").addEventListener("click", dxOpen);
$("dx-close").addEventListener("click", dxClose);
$("dx-finish").addEventListener("click", dxClose);
$("dx-again").addEventListener("click", () => {
  dxMsg("");
  dxShowForm();
  $("dx-go").disabled = false;
  $("dx-go").textContent = "Скачать";
  dxUpdate();
  $("dx-go").focus();
});
$("dx").addEventListener("pointerdown", e => { if (e.target === $("dx")) dxClose(); });
$("dx-go").addEventListener("click", dxRun);
$("dx-help").addEventListener("click", () => {
  const p = $("dx-help-p");
  p.hidden = !p.hidden;
  $("dx-help").setAttribute("aria-expanded", String(!p.hidden));
});
$("dx-all").addEventListener("click", () => {
  dxScope = "all";
  $("dx-all").classList.add("on"); $("dx-one").classList.remove("on");
  $("dx-all").setAttribute("aria-pressed", "true"); $("dx-one").setAttribute("aria-pressed", "false");
  dxMsg("");
  dxUpdate();
});
$("dx-one").addEventListener("click", () => {
  dxScope = "one";
  $("dx-one").classList.add("on"); $("dx-all").classList.remove("on");
  $("dx-one").setAttribute("aria-pressed", "true"); $("dx-all").setAttribute("aria-pressed", "false");
  dxMsg("");
  dxUpdate();
});
for (const inp of document.querySelectorAll('input[name="dxfmt"]')) {
  inp.addEventListener("change", () => {
    dxFmt = inp.value;
    for (const c of document.querySelectorAll(".dx-card")) c.classList.toggle("on", c.dataset.fmt === dxFmt);
    dxMsg("");
    dxUpdate();
  });
}

/* ---------- гайд «Как это работает» ---------- */

let guideLastFocus = null;

function openGuide() {
  guideLastFocus = document.activeElement;
  $("guide").hidden = false;
  $("guide-ok").focus();
}

function closeGuide() {
  if ($("guide").hidden) return;
  $("guide").hidden = true;
  store.set("gostpadi.helpSeen", "1");
  if (guideLastFocus && typeof guideLastFocus.focus === "function") guideLastFocus.focus();
}

$("help").addEventListener("click", openGuide);
$("guide-close").addEventListener("click", closeGuide);
$("guide-ok").addEventListener("click", closeGuide);
$("guide").addEventListener("pointerdown", e => { if (e.target === $("guide")) closeGuide(); });

/* ---------- меню «⋯» и «⚙ параметры» ---------- */

function closeMenus() {
  for (const [b, m] of [
    [$("examples"), $("examples-menu")],
    [$("settings"), $("settings-menu")],
    [$("view-more"), $("view-menu")],
  ]) {
    m.hidden = true;
    b.setAttribute("aria-expanded", "false");
  }
}

function toggleMenu(btn, menu) {
  const open = menu.hidden;
  closeMenus();
  menu.hidden = !open;
  btn.setAttribute("aria-expanded", String(open));
  if (open) menu.querySelector("input, button")?.focus();
}

$("examples").addEventListener("click", e => {
  e.stopPropagation();
  toggleMenu($("examples"), $("examples-menu"));
});
$("settings").addEventListener("click", e => {
  e.stopPropagation();
  toggleMenu($("settings"), $("settings-menu"));
});
$("view-more").addEventListener("click", e => {
  e.stopPropagation();
  toggleMenu($("view-more"), $("view-menu"));
});
addEventListener("pointerdown", e => {
  if (!e.target.closest(".menu-wrap")) closeMenus();
});
for (const m of document.querySelectorAll(".menu")) {
  m.addEventListener("click", e => { if (e.target.closest("button")) closeMenus(); });
}
$("view-full").addEventListener("click", () => openLightbox());

$("copy").addEventListener("click", () => {
  const s = sheets[activeSheet];
  if (!s) return;
  const r = callExport(curItems().filter(it => it.text.trim()), $("ru").checked, +$("lw").value, +$("font").value, 1);
  const f = pickExport(r, s);
  if (!f) return toast("движок не отдал файл");
  navigator.clipboard.writeText(f.sheets[0]).then(() => {
    $("copy").textContent = "скопировано";
    setTimeout(() => { $("copy").textContent = "копировать SVG"; }, 1500);
  }).catch(() => toast("буфер недоступен"));
});

/* Имя файла без расширения: пакет отдаёт «имя.c». */
function stemOf(name) {
  return name.replace(/\.[^./\\]+$/, "").replace(/[^\wа-яёА-ЯЁ -]/g, "_");
}

/* Стрелки листают лист, Esc возвращает в коллаж. Работает, только
   когда открыт просмотр: в коллаже стрелки должны двигать курсор
   по тексту, а не перелистывать схему. */
addEventListener("keydown", e => {
  if (e.key === "Escape") closeMenus();
  if (!$("guide").hidden) {
    if (e.key === "Escape") { closeGuide(); e.preventDefault(); }
    return;
  }
  if (!$("dx").hidden) {
    if (e.key === "Escape") { dxClose(); e.preventDefault(); }
    return;
  }
  if (lbOpen) {
    if (e.key === "Escape") { closeLightbox(); e.preventDefault(); }
    else if (e.key === "ArrowLeft") { lbStep(-1); e.preventDefault(); }
    else if (e.key === "ArrowRight") { lbStep(1); e.preventDefault(); }
    return;
  }
  const t = e.target;
  if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return;
});

/* ============================== файлы ============================== */

function drawFileList() {
  const list = $("filelist");
  list.innerHTML = "";
  if (cur < 0 && !files.length) {
    const li = document.createElement("li");
    li.className = "batchnote";
    li.style.cssText = "padding:6px 10px;display:block";
    li.textContent = "черновик — без имени";
    list.appendChild(li);
    return;
  }
  files.forEach((f, i) => {
    const li = document.createElement("li");
    if (i === cur) li.className = "sel";
    const b = document.createElement("button");
    b.type = "button";
    b.className = "fname";
    b.title = f.name;
    b.textContent = (f.dirty ? "● " : "") + f.name;
    if (badNames.has(f.name)) {
      const dot = document.createElement("span");
      dot.className = "fbadge";
      dot.title = "в файле нет ни одной функции с телом";
      li.appendChild(dot);
    }
    b.addEventListener("click", () => selectFile(i));
    const del = document.createElement("button");
    del.type = "button";
    del.className = "fdel";
    del.textContent = "×";
    del.title = "убрать " + f.name;
    del.setAttribute("aria-label", "убрать " + f.name);
    del.addEventListener("click", e => { e.stopPropagation(); removeFile(i); });
    li.append(b, del);
    list.appendChild(li);
  });
}

function selectFile(i) {
  if (i === cur) return;
  syncCur();
  cur = i;
  $("code").value = files[i].text;
  undoReset();
  updateMode();
  drawFileList();
  persist();
  renderNow();
  if (matchMedia("(max-width: 1080px)").matches) switchTab("view");
}

function removeFile(i) {
  const f = files[i];
  pendingUndo = { name: f.name, files: [...files], cur };
  files.splice(i, 1);
  if (!files.length) cur = -1;
  else {
    cur = Math.max(0, Math.min(cur >= i ? cur - 1 : cur, files.length - 1));
    if (cur >= 0) $("code").value = files[cur].text;
  undoReset();
  }
  drawFileList();
  persist();
  renderNow();
  toast(`удалили «${f.name}» — <button id="undo" class="btn">восстановить</button>`);
}

function undoRemove() {
  if (!pendingUndo) return;
  files = pendingUndo.files;
  cur = pendingUndo.cur;
  pendingUndo = null;
  clearTimeout(undoTimer);
  if (cur >= 0) $("code").value = files[cur].text;
  undoReset();
  drawFileList();
  persist();
  renderNow();
  $("toast").hidden = true;
}

function updateMode() {
  const it = cur >= 0 ? files[cur] : null;
  $("editmode").textContent =
    (it ? it.name : "черновик.c") + " · C" +
    (it && it.dirty ? " · не сохранён" : "");
}

/* добавление файлов: повторное имя обновляет, нечитаемые — в статус */
function uniqName(base, taken) {
  if (!taken.has(base)) return base;
  let i = 2;
  while (taken.has(base.replace(/(\.[^.]*)?$/, `-${i}$1`))) i++;
  return base.replace(/(\.[^.]*)?$/, `-${i}$1`);
}

/* Проверка типа файла при импорте.
   accept=".c,.h,.cpp,.cc,.cxx,.c++,.hpp,.hxx,.hh,.h++,.txt" фильрует только
   диалог выбора: перетаскивание, «выбрать всё» и вставка мимо него
   проходят. Без проверки в пачку попадал бинарник (png, pdf), f.text()
   давал мусор, движок на нём падал — и красная ошибка висела поверх
   старой схемы. */
const ALLOWED_EXT = /\.(c|h|cpp|cc|cxx|c\+\+|hpp|hxx|hh|h\+\+|txt)$/i;

function rejectReason(name, text) {
  if (!ALLOWED_EXT.test(name)) {
    return "не C/C++ или .txt";
  }
  // двоичный файл с правым расширением: f.text() отдаст мусор с NUL
  if (text.includes("\0")) return "похоже на бинарный файл";
  // текст без единого разумного символа C
  if (!/[A-Za-zА-Яа-яЁё_]/.test(text)) return "в файле нет кода";
  return null;
}

async function loadFiles(list) {
  const added = [], failed = [], rejected = [];
  // черновик не выбрасываем: вставленный код становится файлом пачки
  if (cur < 0) {
    const text = $("code").value;
    if (text.trim()) {
      const taken = new Set([...files.map(f => f.name)]);
      added.push({
        name: uniqName("черновик.c", taken),
        stem: "черновик",
        text,
        dirty: false,
      });
    }
  }
  const taken = new Set(files.map(f => f.name));
  const skipped = [];
  for (const f of [...list].slice(0, 64)) {
    try {
      const text = await f.text();
      let name = f.name;
      const why = rejectReason(name, text);
      if (why) { rejected.push(name + " — " + why); continue; }
      // то же имя — не повод затирать: другой код получает соседнее имя
      // (main.c из другой папки → main-2.c), тот же код — пропускаем
      const same = files.find(x => x.name === name) ||
                   added.find(x => x.name === name);
      if (same) {
        if (same.text === text) { skipped.push(name); continue; }
        name = uniqName(name, taken);
      }
      taken.add(name);
      added.push({
        name,
        stem: name.replace(/\.[^.]+$/, "")
          .replace(/[^\wа-яё.-]+/gi, "_") || "scheme",
        text,
        c: /\.c$/i.test(name),
        dirty: false,
      });
    } catch (e) { failed.push(f.name); }
  }
  // Именно toast, а не status: следом идёт renderNow(), и status()
  // перезаписался бы на «готово» — отчёт о пропущенных файлах не
  // дошёл бы до пользователя. Тост живёт 5 секунд поверх схемы.
  if (failed.length) toast("не удалось прочитать: " + failed.join(", "));
  if (rejected.length) toast("пропущено — " + rejected.join("; "));
  if (!added.length) {
    if (skipped.length) status("такой файл уже есть: " + skipped.join(", "));
    return;
  }
  files = files.concat(added);
  cur = files.indexOf(added[0]);
  $("code").value = files[cur].text;
  undoReset();
  badNames = new Set();
  drawFileList();
  updateMode();
  persist();
  renderNow();
  if (matchMedia("(max-width: 1080px)").matches) switchTab("view");
}

$("add").addEventListener("click", () => $("file").click());
$("file").addEventListener("change", e => {
  if (e.target.files.length) loadFiles(e.target.files);
  e.target.value = "";
});
$("clear").addEventListener("click", () => {
  files = []; cur = -1; badNames = new Set();
  $("code").value = "";
  undoReset();
  // чистим все следы: иначе после перезагрузки код вернётся из хэша
  // или localStorage — «очистить всё» перестаёт быть очисткой
  store.del("gostpadi.batch");
  store.del("gostpadi.code");
  history.replaceState(null, "", location.pathname + location.search);
  drawFileList(); updateMode(); renderNow();
});

/* ============================== без тела main ============================== */

/* Ошибку движка — в одну человеческую фразу. Текст парсера
   («неожидаемый токен, ожидалось: ';'») понятен только автору
   парсера; новичку нужны причина и место. */
function errWhy(er, src) {
  if (er.empty) {
    return {
      why: "В этом коде нечего рисовать",
      hint: "Добавь в функцию действия (printf, присваивание, вызов) — схема появится сама.",
    };
  }
  const m = String(er.msg || "");
  const last = src.trimEnd().split("\n").length;
  const where = er.line ? ` строку ${er.line}` : "";
  if (/'}'/.test(m) && (!er.line || er.line >= last)) {
    return {
      why: "Похоже, не закрыта фигурная скобка «}»",
      hint: "Проверь, что каждый открытый { закрыт }.",
    };
  }
  if (/'\)'/.test(m)) {
    return {
      why: "Похоже, не закрыта круглая скобка «)»",
      hint: `Проверь${where} и парность скобок — особенно в printf и if.`,
    };
  }
  if (/'\]'/.test(m)) {
    return {
      why: "Похоже, не закрыта квадратная скобка «]»",
      hint: "Проверь индексы массивов.",
    };
  }
  if (/';\s*'/.test(m)) {
    return {
      why: "Похоже, пропущена «;»",
      hint: er.line
        ? `Проверь конец строки ${er.line} — возможно, там не хватает «;».`
        : "Проверь концы строк — возможно, где-то не хватает «;».",
    };
  }
  if (/не найден.*main/i.test(m)) {
    return {
      why: "Нужна функция main",
      hint: "Добавь int main(void) { ... return 0; } — с неё начинается программа.",
    };
  }
  return {
    why: "Код не удалось разобрать",
    hint: `Проверь${where || " код"} и парность скобок.`,
  };
}

function showErrors(errors, items) {
  const box = $("errbox");
  box.innerHTML = "";
  box.classList.toggle("show", errors.length > 0);
  if (errors.length && !paneOn("editor")) {
    if (isNarrow()) switchTab("edit"); else setPane("editor", true);
  }
  if (!errors.length) { updateMode(); return; }
  for (const er of errors) {
    const idx = items.findIndex(it => it.name === er.name);
    const src = idx >= 0 ? items[idx].text : (er.src || items[0]?.text || "");
    const card = document.createElement("div");
    card.className = "errcard";
    const b = document.createElement("b");
    b.textContent = er.empty
      ? `«${er.name || "код"}»: рисовать нечего`
      : `«${er.name || "код"}»: не получилось нарисовать`;
    card.appendChild(b);

    const line = document.createElement("div");
    line.className = "errline";
    const where = er.line
      ? `строка ${er.line}${er.col ? ", символ " + er.col : ""}`
      : "";
    const why = errWhy(er, src);
    line.textContent = why.why + (where ? " · " + where : "");
    line.title = er.msg || "";
    if (!er.empty) card.appendChild(line);

    const hint = document.createElement("div");
    hint.className = "errhint";
    hint.textContent = why.hint +
      (er.empty ? "" : " Схема перерисуется сама, когда код станет верным.");
    card.appendChild(hint);

    const rows = src.split("\n");
    if (er.line && er.line >= 1 && er.line <= rows.length) {
      const pre = document.createElement("pre");
      pre.title = "нажми — покажу место в коде";
      const to = Math.min(rows.length, er.line + 1);
      const w = String(to).length;
      for (let i = Math.max(1, er.line - 1); i <= to; i++) {
        const row = document.createElement("div");
        row.textContent = String(i).padStart(w) + " | " + (rows[i - 1] ?? "");
        if (i === er.line) row.className = "bad";
        pre.appendChild(row);
      }
      if (er.col > 0) {
        const caret = document.createElement("div");
        caret.textContent = " ".repeat(w) + " | " + " ".repeat(er.col - 1) + "^";
        pre.appendChild(caret);
      }
      pre.addEventListener("click", () => {
        if (idx >= 0 && cur !== idx) selectFile(idx);
        revealLine(er.line, er.col);
      });
      card.appendChild(pre);
    }
    box.appendChild(card);
  }
}

function revealLine(line, col) {
  const ta = $("code");
  const rows = ta.value.split("\n");
  let off = 0;
  for (let i = 0; i < line - 1; i++) off += rows[i].length + 1;
  ta.focus();
  ta.setSelectionRange(off + Math.max(0, (col || 1) - 1), off + (rows[line - 1] || "").length);
  if (matchMedia("(max-width: 1080px)").matches) switchTab("edit");
}

/* ============================== блок схемы → строка кода ============================== */

/* Текст блока ищется в исходнике нормализацией без пробелов/скобок:
   «while quant != 0» находит `while (quant != 0)`, «x=x+5» — `x = x + 5;`.
   Схема — оглавление кода: ткнул в блок, редактор прыгнул на строку. */
const codeNorm = s => s.toLowerCase().replace(/[()\s]/g, "");

function lineForBlock(label) {
  const m = codeNorm(label || "");
  if (m.length < 3 || m.startsWith("#")) return -1;
  const rows = codeTextNow().split("\n");
  let best = -1, bestLen = -1;
  for (let i = 0; i < rows.length; i++) {
    const n = codeNorm(rows[i]);
    if (n.length < 2 || !(n === m || n.includes(m) || m.includes(n))) continue;
    // длина совпадения = уверенность: «sum=sum+1» выигрывает «sum=0»
    const len = Math.min(n.length, m.length);
    if (len > bestLen) { best = i; bestLen = len; }
  }
  return best + 1; // 1-based, 0 если не нашли
}

function jumpToBlock(label) {
  const line = lineForBlock(label);
  if (line <= 0) return false;
  revealLine(line, 1);
  // позиция строки — в видимую треть редактора
  const ta = $("code");
  const lh = parseFloat(getComputedStyle(ta).lineHeight) || 20;
  ta.scrollTop = Math.max(0, (line - 1) * lh - ta.clientHeight / 3);
  return true;
}

let blockDown = null;
function sheetBlockClick(ev) {
  if (blockDown && Math.hypot(ev.clientX - blockDown[0], ev.clientY - blockDown[1]) > 7) return;
  const text = ev.target.closest?.("text");
  // сам .sheet отсекает кнопки тулбара: текст блока бывает только в нём
  if (!text || !ev.target.closest(".sheet")) return;
  if (jumpToBlock(text.textContent)) closeLightbox();
}
$("viewport").addEventListener("pointerdown", e => { blockDown = [e.clientX, e.clientY]; });
$("viewport").addEventListener("click", sheetBlockClick);
$("lbview").addEventListener("pointerdown", e => { blockDown = [e.clientX, e.clientY]; });
$("lbview").addEventListener("click", sheetBlockClick);


/* ============================== undo/redo ============================== */

/* Полная отмена правок кода: нативный undo textarea теряется, как
   только значение выставляет скрипт (файл, пример, ссылка). Стек
   снимков {значение, выделение}: каждый ввод фиксирует прошлое
   состояние, ctrl+z возвращает, ctrl+y /[ctrl+shift+z] повторяет. */
function codeTextNow() {
  return cur >= 0 && files[cur] ? files[cur].text : $("code").value;
}
const undoStack = [], redoStack = [];
let undoLast = null, undoLastSel = [0, 0];
function undoReset() {
  undoStack.length = 0; redoStack.length = 0;
  undoLast = null;
}
function undoTrack() {
  const ta = $("code");
  if (undoLast === null) {
    undoLast = ta.value;
    undoLastSel = [ta.selectionStart, ta.selectionEnd];
    return;
  }
  if (ta.value !== undoLast) {
    undoStack.push({ v: undoLast, s: undoLastSel });
    if (undoStack.length > 200) undoStack.shift();
    redoStack.length = 0;
    undoLast = ta.value;
  }
}
function undoApply(from, to) {
  const ta = $("code");
  undoTrack();
  const snap = from.pop();
  if (!snap) return;
  to.push({ v: ta.value, s: [ta.selectionStart, ta.selectionEnd] });
  ta.value = snap.v;
  undoLast = snap.v;
  try { ta.setSelectionRange(snap.s[0], snap.s[1]); } catch { /* позиция вне текста */ }
  ta.dispatchEvent(new Event("input"));
  ta.focus();
}

/* ============================== сохранение и ссылки ============================== */

function persist() {
  if (files.length) {
    store.set("gostpadi.batch", JSON.stringify({ files, cur }));
    store.del("gostpadi.code");
    if (location.hash) history.replaceState(null, "", location.pathname + location.search);
  } else {
    store.del("gostpadi.batch");
    store.set("gostpadi.code", $("code").value);
  }
}

function b64urlEncode(str) {
  const bytes = new TextEncoder().encode(str);
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}
function b64urlDecode(s) {
  s = s.replace(/-/g, "+").replace(/_/g, "/");
  while (s.length % 4) s += "=";
  const bin = atob(s);
  return new TextDecoder().decode(Uint8Array.from(bin, c => c.charCodeAt(0)));
}

$("share").addEventListener("click", () => {
  syncCur();
  let payload;
  if (files.length) {
    payload = "#j=" + b64urlEncode(JSON.stringify({ files, cur }));
  } else {
    payload = "#c=" + b64urlEncode($("code").value);
  }
  // адрес тоже меняем: можно копировать прямо из строки браузера
  history.replaceState(null, "", location.pathname + location.search + payload);
  const url = location.origin + location.pathname + payload;
  navigator.clipboard.writeText(url).then(() => {
    $("share").textContent = "скопировано";
    setTimeout(() => { $("share").textContent = "ссылка"; }, 1500);
  }).catch(() => {});
});

/* ============================== ввод ============================== */

$("render").addEventListener("click", () => {
  clearTimeout(renderTimer);
  renderNow();
  if (matchMedia("(max-width: 1080px)").matches) switchTab("view");
});
$("ru").addEventListener("change", () => {
  store.set("gostpadi.ru", $("ru").checked ? "1" : "0");
  renderNow();
});
$("lw").addEventListener("input", () => { $("lw-val").textContent = $("lw").value.replace(".", ","); });
$("lw").addEventListener("change", () => { store.set("gostpadi.lw", $("lw").value); renderNow(); });
$("font").addEventListener("input", () => { $("font-val").textContent = $("font").value; });
$("font").addEventListener("change", () => { store.set("gostpadi.font", $("font").value); renderNow(); });

$("code").addEventListener("input", () => {
  undoTrack();
  if (cur < 0) {
    store.set("gostpadi.code", $("code").value);
  } else if (files[cur]) {
    files[cur].text = $("code").value;
    files[cur].dirty = true;
    drawFileList();
  }
  // Пользователь правит код — прошлая ошибка больше не про него.
  // Иначе красная строка про «движок упал» висела бы, пока он
  // дописывает, и выглядела как текущее состояние.
  if ($("statusline").classList.contains("err")) {
    status("правлю код — рисую заново…");
  }
  updateMode();
  if (engineReady) scheduleRender();
});
$("code").addEventListener("keydown", e => {
  if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
    e.preventDefault();
    clearTimeout(renderTimer);
    renderNow();
  }
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z" && !e.shiftKey) {
    e.preventDefault();
    undoApply(undoStack, redoStack);
  }
  if ((e.ctrlKey || e.metaKey)
      && (e.key.toLowerCase() === "y" || (e.shiftKey && e.key.toLowerCase() === "z"))) {
    e.preventDefault();
    undoApply(redoStack, undoStack);
  }
});

for (const b of document.querySelectorAll(".ex")) {
  b.addEventListener("click", () => {
    const text = EXAMPLES[b.dataset.ex];
    if (!text) return;
    files = [];
    cur = -1;
    badNames = new Set();
    drawFileList();
    store.del("gostpadi.batch");
    $("code").value = text;
    undoReset();
    updateMode();
    store.set("gostpadi.code", text);
    renderNow();
  });
}

/* перетаскивание на всё окно */
let dragDepth = 0;
addEventListener("dragenter", e => {
  e.preventDefault();
  if (e.dataTransfer?.types.includes("Files") && ++dragDepth === 1) $("dragover").hidden = false;
});
addEventListener("dragover", e => e.preventDefault());
addEventListener("dragleave", e => {
  e.preventDefault();
  if (--dragDepth <= 0) { dragDepth = 0; $("dragover").hidden = true; }
});
addEventListener("drop", e => {
  e.preventDefault();
  dragDepth = 0;
  $("dragover").hidden = true;
  if (e.dataTransfer?.files.length) loadFiles(e.dataTransfer.files);
});

/* мобильные табы + опциональные панели десктопа */
function isNarrow() { return matchMedia("(max-width: 1080px)").matches; }

function switchTab(name) {
  document.body.dataset.mtab = name;
  for (const b of $("tabbar").children) {
    b.setAttribute("aria-current", b.dataset.tab === name ? "true" : "false");
  }
  // панель только что показана: размеры доступны после следующего кадра.
  // Без этого масштаб считался по скрытой панели и ломался.
  requestAnimationFrame(() => {
    if (zoom === null) applyZoom();
    // коллаж пересчитывается от ширины панели: пока вкладка была
    // скрыта, ширина нулевая, и лист выходил в 180 px
    const st = $("stage");
    if (st.classList.contains("collage") && ownSheets().length && st.clientWidth !== collageW) drawSheets();
  });
}

/* десктоп: какие панели показаны; схема видна всегда */
function paneOn(name) { return document.body.dataset[name] !== "off"; }
function setPane(name, on) {
  document.body.dataset[name] = on ? "on" : "off";
  store.set("gostpadi.panes", JSON.stringify({
    files: paneOn("files"), editor: paneOn("editor"),
  }));
  syncChips();
}
function syncChips() {
  $("vt-files").setAttribute("aria-pressed", paneOn("files"));
  $("vt-editor").setAttribute("aria-pressed", paneOn("editor"));
}
$("vt-files").addEventListener("click", () => setPane("files", !paneOn("files")));
$("vt-editor").addEventListener("click", () => setPane("editor", !paneOn("editor")));

/* Кнопки нижнего бара на телефоне. switchTab писали и вызывали из кода
   (переход на схему после рендера), но обработчика на сам клик не
   было: бар выглядел рабочим и не реагировал. */
for (const b of $("tabbar").children) {
  b.addEventListener("click", () => switchTab(b.dataset.tab));
}
(function initPanes() {
  let saved = null;
  try { saved = JSON.parse(store.get("gostpadi.panes") || "null"); } catch (e) {}
  document.body.dataset.files = saved?.files === false ? "off" : "on";
  document.body.dataset.editor = saved?.editor === false ? "off" : "on";
  syncChips();
})();

/* «+» — сразу выбор файла; каждый выбранный/брошенный файл
   становится отдельной записью пачки */
$("add").addEventListener("click", () => $("file").click());

/* ============================== старт ============================== */

/* палитра команд: ctrl+k где угодно; стрелки/Enter — навигация */
const COMMANDS = [
  { t: "скачать", hint: "SVG, PNG или draw.io", run: () => $("dx-open").click() },
  { t: "параметры схемы", hint: "надписи, линии, кегль", run: () => $("settings").click() },
  { t: "пример: проверка ввода", hint: "scanf и ветвление", run: () => document.querySelector(".ex[data-ex='input']")?.click() },
  { t: "пример: цикл", hint: "for и while", run: () => document.querySelector(".ex[data-ex='loop']")?.click() },
  { t: "пример: switch", hint: "выбор по значению", run: () => document.querySelector(".ex[data-ex='switch']")?.click() },
  { t: "копировать SVG", hint: "в буфер обмена", run: () => $("copy").click() },
  { t: "скопировать ссылку", hint: "сессия в адресной строке", run: () => $("share").click() },
  { t: "сменить тему", hint: "система → светлая → тёмная", run: () => $("theme").click() },
  { t: "очистить всё", hint: "пачка и черновик", run: () => $("clear").click() },
];
let pRows = [], pSel = 0;

function pPaint() {
  const li = $("plist");
  li.innerHTML = "";
  pRows.forEach((c, i) => {
    const el = document.createElement("li");
    if (i === pSel) el.classList.add("sel");
    el.textContent = c.t;
    if (c.hint) {
      const sm = document.createElement("small");
      sm.textContent = c.hint;
      el.appendChild(sm);
    }
    el.addEventListener("click", () => { paletteClose(); c.run(); });
    li.appendChild(el);
  });
}
function pFilter(q) {
  const words = q.toLowerCase().split(/\s+/).filter(Boolean);
  pRows = COMMANDS.filter(c => words.every(w => (c.t + " " + (c.hint || "")).toLowerCase().includes(w)));
  pSel = 0;
  pPaint();
}
function paletteOpen() {
  const p = $("palette");
  p.hidden = false;
  const i = $("pinput");
  i.value = "";
  pFilter("");
  i.focus();
}
function paletteClose() { $("palette").hidden = true; }
$("pinput").addEventListener("input", () => pFilter($("pinput").value));
$("pinput").addEventListener("keydown", e => {
  if (e.key === "ArrowDown") { pSel = Math.min(pSel + 1, pRows.length - 1); pPaint(); e.preventDefault(); }
  if (e.key === "ArrowUp") { pSel = Math.max(pSel - 1, 0); pPaint(); e.preventDefault(); }
  if (e.key === "Enter" && pRows[pSel]) { paletteClose(); pRows[pSel].run(); }
  if (e.key === "Escape") { paletteClose(); e.preventDefault(); }
});
$("palette").addEventListener("pointerdown", e => { if (e.target.id === "palette") paletteClose(); });
addEventListener("keydown", e => {
  // модальные слои: Ctrl+K под ними открыл бы палитру, которую не видно
  if (!$("dx").hidden || !$("guide").hidden) return;
  if ((e.ctrlKey || e.metaKey) && !e.shiftKey && e.key.toLowerCase() === "k") {
    e.preventDefault();
    if ($("palette").hidden) paletteOpen(); else paletteClose();
  } else if (e.key === "Escape" && !$("palette").hidden) {
    paletteClose();
  }
});

$("ph-ex").addEventListener("click", () => {
  document.querySelector(".ex[data-ex='input']")?.click();
});

/* Онбординг — один раз на браузер: дальше пустое состояние говорит
   делом, а не экскурсией. */


/* Подсказки в статус-строке кликабельные: это те же команды, что по
   горячим клавишам, но их видно и можно нажать пальцем. */
for (const b of document.querySelectorAll(".hintbtn")) {
  b.addEventListener("click", () => {
    const act = b.dataset.act;
    if (act === "render") { clearTimeout(renderTimer); renderNow(); }
    else if (act === "undo") undoApply(undoStack, redoStack);
    else if (act === "palette") paletteOpen();
  });
}

/* Короткие программы под рукой: выбрал — код подставился и нарисовался.
   Без глобальных массивов и argc/argv: это примеры, а не учебник. */
const EXAMPLES = {
  branch: `#include <stdio.h>

int main(void) {
    int x;
    printf("x = ");
    scanf("%d", &x);
    if (x > 0) {
        printf("positive\\n");
    } else if (x < 0) {
        printf("negative\\n");
    } else {
        printf("zero\\n");
    }
    return 0;
}`,
  loop: `#include <stdio.h>

int main(void) {
    int i, sum = 0;
    for (i = 1; i <= 5; i++) {
        sum = sum + i;
    }
    while (sum > 10) {
        sum = sum - 10;
    }
    printf("sum = %d\\n", sum);
    return 0;
}`,
  switch: `#include <stdio.h>

int main(void) {
    int day;
    scanf("%d", &day);
    switch (day) {
    case 1: printf("Mon\\n"); break;
    case 2: printf("Tue\\n"); break;
    case 3: printf("Wed\\n"); break;
    default: printf("Other\\n"); break;
    }
    return 0;
}`,
  array: `#include <stdio.h>

int main(void) {
    int a[5];
    int i, sum = 0;
    a[0] = 3;
    a[1] = 1;
    a[2] = 4;
    a[3] = 1;
    a[4] = 5;
    for (i = 0; i < 5; i++) {
        sum = sum + a[i];
    }
    printf("sum = %d\\n", sum);
    return 0;
}`,
  funcs: `#include <stdio.h>

int max(int a, int b) {
    if (a > b) {
        return a;
    }
    return b;
}

int main(void) {
    int x, y;
    scanf("%d %d", &x, &y);
    printf("max = %d\\n", max(x, y));
    return 0;
}`,
  input: EXAMPLE_C,
};

(function restore() {
  const hm = location.hash.match(/[#&]c=([A-Za-z0-9_-]+)/);
  const hj = location.hash.match(/[#&]j=([A-Za-z0-9_-]+)/);
  if (hj) {
    try {
      const saved = JSON.parse(b64urlDecode(hj[1]));
      if (Array.isArray(saved.files) && saved.files.length) {
        files = saved.files;
        cur = Math.min(Math.max(saved.cur | 0, 0), files.length - 1);
        $("code").value = files[cur].text;
  undoReset();
      }
    } catch (e) { /* битая ссылка — стартуем с примера */ }
  } else if (hm) {
    $("code").value = b64urlDecode(hm[1]);
    undoReset();
  } else {
    try {
      const saved = JSON.parse(store.get("gostpadi.batch") || "null");
      if (Array.isArray(saved?.files) && saved.files.length) {
        files = saved.files;
        cur = Math.min(Math.max(saved.cur | 0, 0), files.length - 1);
        $("code").value = files[cur].text;
  undoReset();
      }
    } catch (e) { /* битый кеш — начнём с примера */ }
    if (cur < 0 && !$("code").value) {
      $("code").value = store.get("gostpadi.code") || "";
      undoReset();
    }
  }
  $("ru").checked = store.get("gostpadi.ru") !== "0";
  if (store.get("gostpadi.lw")) $("lw").value = store.get("gostpadi.lw");
  $("lw-val").textContent = $("lw").value.replace(".", ",");
  if (store.get("gostpadi.font")) $("font").value = store.get("gostpadi.font");
  $("font-val").textContent = $("font").value;
  updateMode();
  drawFileList();
  if (matchMedia("(max-width: 1080px)").matches) switchTab("edit");
})();

/* гайд: один раз при первом визите — пустой редактор сам объясняет, что делать */
if (!store.get("gostpadi.helpSeen")) {
  openGuide();
}

const loadbar = $("loadbar");
const loadbarT = setTimeout(() => loadbar.classList.add("on"), 250);
loadEngine().catch(e => {
  status("движок не загрузился: " + e.message, true);
}).finally(() => {
  clearTimeout(loadbarT);
  loadbar.classList.add("done");
});
