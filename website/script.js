"use strict";

// The preview demonstrates fixed examples; file comparison runs in the desktop app.
let demoMode = "diff";
let diffApplied = false;
let mergeChoice = null;

const byId = (id) => document.getElementById(id);
const status = byId("demo-status");

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
  byId("merge-comment").textContent = resolved ? `// ${mergeChoice === "source" ? "Source" : "Target"} 적용 · 충돌 해결됨` : "// Base · 충돌을 해결해 주세요";
  document.querySelectorAll("[data-merge-choice]").forEach((button) => {
    button.setAttribute("aria-pressed", String(button.dataset.mergeChoice === mergeChoice));
  });

  status.classList.toggle("is-conflict", !isDiff && !resolved);
  const message = isDiff
    ? diffApplied ? "동일한 파일 · 변경 사항 없음" : "2줄 변경 / 1개 블록"
    : resolved ? "충돌 해결됨 · 결과 적용 완료" : "1개 충돌 / Base에서 시작";
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
  textarea.setAttribute("aria-label", "복사할 명령어");
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
    const label = button.querySelector("span");
    if (copied) {
      label.textContent = "복사 완료";
      byId("copy-status").textContent = `${button.getAttribute("aria-label")} 완료`;
    } else {
      const selection = window.getSelection();
      const range = document.createRange();
      range.selectNodeContents(code);
      selection.removeAllRanges();
      selection.addRange(range);
      label.textContent = "Ctrl+C로 복사";
      byId("copy-status").textContent = "명령어를 선택했습니다. Ctrl+C를 눌러 복사하세요.";
    }
    clearTimeout(button.copyResetTimer);
    button.copyResetTimer = setTimeout(() => { label.textContent = "복사"; }, 2200);
  });
});

updateDemo();
