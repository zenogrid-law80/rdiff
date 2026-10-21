"use strict";

// The preview demonstrates fixed examples; file comparison runs in the desktop app.
let demoMode = "diff";
let diffApplied = false;
let mergeChoice = null;

const byId = (id) => document.getElementById(id);
const status = byId("demo-status");
const i18n = window.SiteI18n;
const t = (key, values) => i18n.t(key, values);
let activeCopyButton = null;

function updatePreferences() {
  byId("language-select").value = i18n.language;
  document.querySelectorAll("button[data-theme]").forEach((button) => {
    button.setAttribute("aria-pressed", String(button.dataset.theme === i18n.theme));
  });
}

function updateCopyFeedback() {
  document.querySelectorAll("[data-copy]").forEach((button) => {
    button.querySelector("span").textContent = t(button.dataset.copyState === "done" ? "copy.done" : button.dataset.copyState === "manual" ? "copy.manual" : "copy");
  });
  const active = activeCopyButton?.dataset.copyState ? activeCopyButton : null;
  byId("copy-status").textContent = active ? active.dataset.copyState === "done"
    ? t("copy.success", { command: active.getAttribute("aria-label") }) : t("copy.fallback") : "";
}

function translatePage() {
  for (const [binding, attribute] of [["data-i18n", null], ["data-i18n-html", "html"], ["data-i18n-aria", "aria-label"], ["data-i18n-title", "title"], ["data-i18n-content", "content"]]) {
    document.querySelectorAll(`[${binding}]`).forEach((element) => {
      const translated = t(element.getAttribute(binding));
      if (attribute === "html") element.innerHTML = translated;
      else if (attribute) element.setAttribute(attribute, translated);
      else element.textContent = translated;
    });
  }
  document.querySelector('meta[property="og:locale"]').setAttribute("content", { ko: "ko_KR", en: "en_US", "zh-Hant": "zh_TW" }[i18n.language]);
  updatePreferences();
  updateDemo();
  updateCopyFeedback();
}

byId("language-select").addEventListener("change", (event) => i18n.setLanguage(event.target.value));
document.querySelectorAll("button[data-theme]").forEach((button) => {
  button.addEventListener("click", () => i18n.setTheme(button.dataset.theme));
});
window.addEventListener("rdiff:languagechange", translatePage);
window.addEventListener("rdiff:themechange", updatePreferences);

function updateDemo() {
  const isDiff = demoMode === "diff";
  byId("diff-panel").hidden = !isDiff;
  byId("merge-panel").hidden = isDiff;
  byId("diff-actions").hidden = !isDiff;
  byId("merge-actions").hidden = isDiff;
  document.querySelectorAll("[data-demo-tab]").forEach((tab) => {
    const selected = tab.dataset.demoTab === demoMode;
    tab.classList.toggle("active", selected);
    tab.setAttribute("aria-selected", String(selected));
    tab.tabIndex = selected ? 0 : -1;
  });

  byId("diff-retries").textContent = diffApplied ? "3" : "5";
  byId("diff-timeout").textContent = diffApplied ? "1000" : "3000";
  byId("diff-panel").classList.toggle("is-applied", diffApplied);
  document.querySelectorAll("[data-diff-highlight]").forEach((line) => {
    const original = line.parentElement.dataset.diffSide === "original";
    line.classList.toggle("diff-old", !diffApplied && original);
    line.classList.toggle("diff-new", !diffApplied && !original);
    line.querySelector("span").classList.toggle("token-old", !diffApplied && original);
    line.querySelector("span").classList.toggle("token-new", !diffApplied && !original);
  });
  byId("apply-diff").disabled = diffApplied;

  const resolved = mergeChoice !== null;
  byId("merge-result").textContent = mergeChoice === "source" ? "5" : mergeChoice === "target" ? "8" : "3";
  byId("merge-result-line").classList.toggle("conflict-line", !resolved);
  byId("merge-result-line").classList.toggle("diff-new", resolved);
  byId("merge-result").className = resolved ? "token-new" : "token-conflict";
  byId("merge-comment").textContent = resolved ? t("demo.commentResolved", { side: mergeChoice === "source" ? "Source" : "Target" }) : t("demo.commentBase");
  document.querySelectorAll("[data-merge-choice]").forEach((button) => {
    button.setAttribute("aria-pressed", String(button.dataset.mergeChoice === mergeChoice));
  });

  status.classList.toggle("is-conflict", !isDiff && !resolved);
  const message = isDiff
    ? t(diffApplied ? "demo.noChanges" : "demo.changes")
    : t(resolved ? "demo.resolved" : "demo.conflict");
  status.replaceChildren();
  const dot = document.createElement("span");
  dot.className = "status-dot";
  status.append(dot, document.createTextNode(message));
}

document.querySelectorAll("[data-demo-tab]").forEach((tab) => {
  tab.addEventListener("click", () => {
    demoMode = tab.dataset.demoTab;
    updateDemo();
  });
});

byId("apply-diff").addEventListener("click", () => {
  diffApplied = true;
  updateDemo();
});

document.querySelectorAll("[data-merge-choice]").forEach((button) => {
  button.addEventListener("click", () => {
    mergeChoice = button.dataset.mergeChoice;
    updateDemo();
  });
});

byId("reset-demo").addEventListener("click", () => {
  if (demoMode === "diff") diffApplied = false;
  else mergeChoice = null;
  updateDemo();
});

document.querySelectorAll("[data-command-tab]").forEach((tab) => {
  tab.addEventListener("click", () => {
    const selectedMode = tab.dataset.commandTab;
    document.querySelectorAll("[data-command-tab]").forEach((item) => {
      const selected = item.dataset.commandTab === selectedMode;
      item.classList.toggle("active", selected);
      item.setAttribute("aria-selected", String(selected));
      item.tabIndex = selected ? 0 : -1;
      byId(`${item.dataset.commandTab}-panel`).hidden = !selected;
    });
  });
});

// Arrow keys follow the WAI-ARIA tabs pattern; Tab leaves the tab list.
document.querySelectorAll('[role="tablist"]').forEach((list) => {
  list.addEventListener("keydown", (event) => {
    const tabs = Array.from(list.querySelectorAll('[role="tab"]'));
    const index = tabs.indexOf(document.activeElement);
    if (index === -1) return;
    let next;
    if (event.key === "ArrowRight") next = (index + 1) % tabs.length;
    else if (event.key === "ArrowLeft") next = (index - 1 + tabs.length) % tabs.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = tabs.length - 1;
    else return;
    event.preventDefault();
    tabs[next].focus();
    tabs[next].click();
  });
});

async function copyText(text) {
  if (navigator.clipboard && window.isSecureContext) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch {
      // Local files and browser permission settings can require the fallback.
    }
  }
  const previousFocus = document.activeElement;
  const textarea = document.createElement("textarea");
  textarea.value = text;
  textarea.className = "sr-only";
  textarea.setAttribute("aria-label", t("copy.command"));
  document.body.append(textarea);
  textarea.select();
  let copied = false;
  try {
    copied = document.execCommand("copy");
  } finally {
    textarea.remove();
    previousFocus?.focus({ preventScroll: true });
  }
  return copied;
}

document.querySelectorAll("[data-copy]").forEach((button) => {
  button.addEventListener("click", async () => {
    const code = byId(button.dataset.copy);
    let copied = false;
    try {
      copied = await copyText(code.textContent);
    } catch {
      copied = false;
    }
    if (copied) {
      button.dataset.copyState = "done";
    } else {
      const selection = window.getSelection();
      const range = document.createRange();
      range.selectNodeContents(code);
      selection.removeAllRanges();
      selection.addRange(range);
      button.dataset.copyState = "manual";
    }
    activeCopyButton = button;
    updateCopyFeedback();
    clearTimeout(button.copyResetTimer);
    button.copyResetTimer = setTimeout(() => { delete button.dataset.copyState; updateCopyFeedback(); }, 2200);
  });
});

translatePage();
