// воркер: Pyodide и движок живут здесь, страница не подвисает при рендере
importScripts("https://cdn.jsdelivr.net/pyodide/v0.26.4/full/pyodide.js");

const SRC_URL =
  "https://raw.githubusercontent.com/sehaxe/gostpadi/main/gostpadi.py";

let pyodide = null;

const RENDER = `
import json, base64, os, re
import gostpadi

out = {}
try:
    # если это код C — сначала переводим в текст схемы
    src = code_text
    if "#include" in src or re.search(r"\\bint\\s+main\\s*\\(", src):
        src = gostpadi.c_to_gvn(src, labels=labels)

    stem = "/tmp/" + stem
    pngs = gostpadi.render(src, stem + ".png", labels=labels, edge_lw=lw)
    svgs = gostpadi.render(src, stem + ".svg", labels=labels, edge_lw=lw)
    sheets = []
    for png, svg in zip(pngs, svgs):
        name = os.path.basename(png)[:-4]
        sheets.append({
            "name": name,
            "png": base64.b64encode(open(png, "rb").read()).decode(),
            "svg": base64.b64encode(open(svg, "rb").read()).decode(),
        })
    out = {"ok": True, "sheets": sheets}
except gostpadi.ParseError as e:
    # ошибка с местом: движок знает строку, столбец и саму строку кода
    out = {"ok": False, "error":
           {"msg": e.msg, "line": e.line, "col": e.col, "src": e.src}}
json.dumps(out)
`;

async function boot() {
  post({ stage: "Загружаю Python (Pyodide)…" });
  pyodide = await loadPyodide();
  post({ stage: "Загружаю matplotlib…" });
  await pyodide.loadPackage("matplotlib");
  post({ stage: "Загружаю pycparser…" });
  await pyodide.loadPackage("pycparser");
  post({ stage: "Загружаю движок gostpadi…" });
  let src = null;
  for (const url of ["../gostpadi.py", SRC_URL]) {
    try {
      const r = await fetch(url);
      if (r.ok) { src = await r.text(); break; }
    } catch (e) { /* пробуем следующий адрес */ }
  }
  if (src === null) throw new Error("не удалось получить gostpadi.py");
  pyodide.FS.writeFile("/gostpadi.py", src, { encoding: "utf8" });
  pyodide.runPython("import sys; sys.path.insert(0, '/')");
  pyodide.runPython("import gostpadi");
  post({ ready: true });
}

function post(m) { self.postMessage(m); }

const fatal = (e) =>
  post({ fatal: (e && e.message) ? e.message : String(e) });

self.onmessage = (e) => {
  const m = e.data;
  if (!m.render) return;
  const t0 = performance.now();
  pyodide.globals.set("code_text", m.code);
  pyodide.globals.set("labels", m.labels);
  pyodide.globals.set("lw", m.lw);
  pyodide.globals.set("stem", m.stem);
  try {
    const res = JSON.parse(pyodide.runPython(RENDER));
    res.seq = m.seq;  // страница отбрасывает ответы без seq (устаревшие)
    res.sec = ((performance.now() - t0) / 1000).toFixed(1).replace(".", ",");
    post(res);
  } catch (err) {
    fatal(err);
  }
};

boot().catch(fatal);
