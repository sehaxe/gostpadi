#!/usr/bin/env node
// Собирает самодостаточный docs/app.html из web/: правится исходник
// web/index.html + web/style.css + web/main.js, а страница для деплоя —
// один файл с вложенными CSS и JS. Зависимостей нет.
// Запуск: node web/build.mjs
import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

const webDir = path.dirname(path.resolve(process.argv[1]));
const rootDir = path.resolve(webDir, "..");
const read = (name) => readFileSync(path.join(webDir, name), "utf8");

const CSS_LINK = '<link rel="stylesheet" href="./style.css">';
const JS_SRC = '<script src="./main.js"></script>';

// Вставляет ровно один блок на место плейсхолдера; плейсхолдер обязан
// встречаться один раз, иначе сборка молча потеряет CSS или JS.
function inlineOnce(html, needle, open, close, body) {
  const at = html.indexOf(needle);
  if (at === -1) throw new Error(`web/index.html: не найден ${needle}`);
  if (html.indexOf(needle, at + needle.length) !== -1) {
    throw new Error(`web/index.html: ${needle} встречается больше одного раза`);
  }
  const text = `${open}\n${body.endsWith("\n") ? body : `${body}\n`}${close}`;
  return html.slice(0, at) + text + html.slice(at + needle.length);
}

const count = (text, needle) => text.split(needle).length - 1;

let html = read("index.html");
html = inlineOnce(html, CSS_LINK, "<style>", "</style>", read("style.css"));
html = inlineOnce(html, JS_SRC, "<script>", "</script>", read("main.js"));

if (count(html, "<style") !== 1 || count(html, "</style>") !== 1) {
  throw new Error("docs/app.html: ожидался ровно один блок <style>");
}
if (count(html, "<script") !== 1 || count(html, "</script>") !== 1) {
  throw new Error("docs/app.html: ожидался ровно один блок <script>");
}

const out = path.join(rootDir, "docs", "app.html");
writeFileSync(out, html);
const lines = count(html, "\n");
console.log(`docs/app.html ← web/ (${Buffer.byteLength(html)} байт, ${lines} строк)`);
