"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const source = fs.readFileSync(path.join(__dirname, "../website/i18n.js"), "utf8");
const html = fs.readFileSync(path.join(__dirname, "../website/index.html"), "utf8");

function boot({ languages = ["en-US"], saved = {}, dark = false, blocked = false } = {}) {
  const storage = new Map(Object.entries(saved));
  const root = { dataset: {}, style: {} };
  const media = { matches: dark, addEventListener: (_, listener) => { media.change = listener; } };
  const events = [];
  const meta = {};
  const window = {
    localStorage: {
      getItem(key) { if (blocked) throw Error("Storage blocked"); return storage.get(key) ?? null; },
      setItem(key, value) { if (blocked) throw Error("Storage blocked"); storage.set(key, value); }
    },
    matchMedia: () => media,
    dispatchEvent: (event) => events.push(event.type)
  };
  vm.runInNewContext(source, {
    window,
    document: { documentElement: root, querySelector: () => ({ setAttribute: (name, value) => { meta[name] = value; } }) },
    navigator: { languages, language: languages[0] },
    CustomEvent: class { constructor(type) { this.type = type; } }
  });
  return { api: window.SiteI18n, root, storage, events, meta, systemChange(value) { media.matches = value; media.change(); } };
}

test("detects the first supported browser language and maps Chinese to Traditional Chinese", () => {
  for (const [languages, expected] of [
    [["ko-KR"], "ko"], [["fr-FR", "en-GB"], "en"], [["ja", "ko"], "ko"],
    [["zh-TW"], "zh-Hant"], [["zh-HK"], "zh-Hant"], [["zh-CN"], "zh-Hant"], [["ja-JP"], "en"]
  ]) assert.equal(boot({ languages }).root.lang, expected);
});

test("restores saved language and appearance before rendering", () => {
  const { api, root, meta } = boot({ saved: { "rdiff.website.language": "zh-Hant", "rdiff.website.theme": "light" }, dark: true });
  assert.equal(api.language, "zh-Hant");
  assert.equal(root.lang, "zh-Hant");
  assert.equal(root.dataset.theme, "light");
  assert.equal(root.style.colorScheme, "light");
  assert.equal(meta.content, "#f7f9f5");
});

test("system appearance follows live changes; explicit themes stay fixed", () => {
  const context = boot();
  assert.equal(context.api.theme, "system");
  assert.equal(context.root.dataset.theme, "light");
  context.systemChange(true);
  assert.equal(context.root.dataset.theme, "dark");
  context.api.setTheme("light");
  context.systemChange(false);
  context.systemChange(true);
  assert.equal(context.root.dataset.theme, "light");
  context.api.setTheme("dark");
  context.systemChange(false);
  assert.equal(context.root.dataset.theme, "dark");
  context.api.setTheme("system");
  assert.equal(context.root.dataset.theme, "light");
});

test("preferences persist independently and emit UI updates", () => {
  const context = boot();
  context.api.setTheme("dark");
  context.api.setLanguage("ko");
  assert.equal(context.storage.get("rdiff.website.theme"), "dark");
  assert.equal(context.storage.get("rdiff.website.language"), "ko");
  assert.equal(context.root.dataset.appearance, "dark");
  assert.deepEqual(context.events, ["rdiff:themechange", "rdiff:languagechange"]);
  const next = boot({ saved: Object.fromEntries(context.storage) });
  assert.equal(next.api.language, "ko");
  assert.equal(next.api.theme, "dark");
});

test("invalid stored preferences and invalid updates fall back safely", () => {
  const context = boot({ languages: ["ko-KR"], saved: { "rdiff.website.language": "<script>", "rdiff.website.theme": "purple" } });
  context.api.setLanguage("unsupported");
  context.api.setTheme("unsupported");
  assert.equal(context.api.language, "ko");
  assert.equal(context.api.theme, "system");
  assert.equal(context.events.length, 0);
});

test("blocked storage still permits language and theme changes", () => {
  const context = boot({ blocked: true });
  context.api.setLanguage("zh-Hant");
  context.api.setTheme("dark");
  assert.equal(context.root.lang, "zh-Hant");
  assert.equal(context.root.dataset.theme, "dark");
  assert.equal(context.api.t("demo.commentResolved", { side: "Source" }), "// 已套用 Source · 衝突已解決");
});

test("every bound message has all three translations, including metadata and accessibility", () => {
  const { api } = boot();
  const keys = [...html.matchAll(/data-i18n(?:-html|-aria|-title|-content)?="([^"]+)"/g)].map(match => match[1]);
  assert.ok(keys.length > 100);
  for (const key of keys) assert.ok(api.messages[key], `Missing bound key: ${key}`);
  for (const [key, values] of Object.entries(api.messages)) {
    assert.equal(values.length, 3, key);
    assert.ok(values.every(value => typeof value === "string" && value.length), key);
    for (const value of values.slice(1)) {
      assert.doesNotMatch(value, /[가-힣]/, key);
      assert.deepEqual([...value.matchAll(/\{(\w+)\}/g)].map(match => match[1]).sort(), [...values[0].matchAll(/\{(\w+)\}/g)].map(match => match[1]).sort(), key);
    }
  }
});

test("translations preserve CLI paths and download destinations", () => {
  const { api } = boot();
  for (const key of ["command.note", "command.p4vNote", "faq.updatesBody"]) {
    const examples = api.messages[key].map(text => [...text.matchAll(/<code>(.*?)<\/code>/g)].map(match => match[1]));
    assert.deepEqual(examples[1], examples[0], key);
    assert.deepEqual(examples[2], examples[0], key);
  }
  for (const key of ["start.downloadBody", "faq.updatesBody"]) {
    const links = api.messages[key].map(text => [...text.matchAll(/href="([^"]+)"/g)].map(match => match[1]));
    assert.deepEqual(links[1], links[0], key);
    assert.deepEqual(links[2], links[0], key);
  }
  assert.match(html, /<script src="\.\/i18n\.js"><\/script>\s*<link rel="stylesheet"/);
  assert.equal([...html.matchAll(/<img src="\.\/favicon\.png"/g)].length, 3);
});
