"use strict";

// Rich markup is authored here; user input is never inserted as HTML.
const siteMessages = Object.freeze({
  "meta.title": ["rdiff — 차이는 선명하게. 병합은 자연스럽게.", "rdiff — Clearer changes. Smoother merges.", "rdiff — 清楚比較，順暢合併。"],
  "meta.description": ["rdiff는 Windows와 macOS를 위한 네이티브 텍스트 비교·3-way 병합 도구입니다. 변경 내용을 나란히 비교하고, 직접 편집하고, P4V 워크플로에 연결하세요.", "rdiff is a native text comparison and three-way merge tool for Windows and macOS. Compare changes side by side, edit directly, and connect it to your P4V workflow.", "rdiff 是 Windows 與 macOS 的原生文字比較及三方合併工具。並排比較變更、直接編輯，並整合至 P4V 工作流程。"],
  "meta.social": ["텍스트 비교부터 3-way 병합까지. Rust와 GPUI로 만든 Windows·macOS 네이티브 도구.", "From text comparison to three-way merging. A native Windows and macOS tool built with Rust and GPUI.", "從文字比較到三方合併，以 Rust 與 GPUI 打造的 Windows、macOS 原生工具。"],
  "skip": ["본문으로 건너뛰기", "Skip to content", "跳至主要內容"],
  "home": ["rdiff 홈", "rdiff home", "rdiff 首頁"],
  "nav.main": ["주 메뉴", "Main navigation", "主要導覽"],
  "nav.features": ["주요 기능", "Features", "功能"],
  "nav.start": ["시작하기", "Get started", "開始使用"],
  "nav.faq": ["FAQ", "FAQ", "常見問題"],
  "nav.github": ["GitHub 저장소", "GitHub repository", "GitHub 儲存庫"],
  "nav.download": ["다운로드", "Download", "下載"],
  "nav.footer": ["하단 메뉴", "Footer navigation", "頁尾導覽"],
  "language": ["언어", "Language", "語言"],
  "appearance": ["테마", "Appearance", "外觀"],
  "theme.system": ["시스템 설정", "System", "系統設定"],
  "theme.light": ["밝은 테마", "Light", "淺色"],
  "theme.dark": ["어두운 테마", "Dark", "深色"],
  "hero.eyebrow": ["A CLEARER VIEW OF CHANGE", "A CLEARER VIEW OF CHANGE", "讓每一處變更更清楚"],
  "hero.platform": ["NATIVE FOR WINDOWS & MACOS", "NATIVE FOR WINDOWS & MACOS", "WINDOWS 與 MACOS 原生應用程式"],
  "release.latest": ["최신 버전", "Latest release", "最新版本"],
  "hero.title": ["차이는 선명하게.<br>병합은 <span>자연스럽게.</span>", "Clearer changes.<br><span>Smoother merges.</span>", "清楚比較，<br><span>順暢合併。</span>"],
  "hero.intro": ["두 파일 사이의 작은 차이부터<br class=\"desktop-break\"> 세 갈래로 나뉜 코드의 충돌까지.<br>비교하고, 편집하고, 하나로 연결하세요.", "From small differences between two files<br class=\"desktop-break\"> to conflicts across three versions.<br>Compare, edit, and bring it all together.", "從兩個檔案的細微差異，<br class=\"desktop-break\">到三個版本之間的程式碼衝突。<br>比較、編輯，讓變更合而為一。"],
  "download.windows": ["Windows 다운로드", "Download for Windows", "下載 Windows 版"],
  "download.mac": ["macOS DMG 다운로드", "Download macOS DMG", "下載 macOS DMG"],
  "download.installer": ["Windows 설치 파일 선택", "Choose a Windows installer", "選擇 Windows 安裝程式"],
  "download.portable": ["포터블 EXE 다운로드", "Download portable EXE", "下載可攜式 EXE"],
  "download.free": ["무료 오픈 소스", "Free and open source", "免費且開放原始碼"],
  "demo.label": ["INTERACTIVE PREVIEW", "INTERACTIVE PREVIEW", "互動預覽"],
  "demo.words": ["단어", "Words", "單字"],
  "demo.sync": ["동기화 스크롤", "Sync scroll", "同步捲動"],
  "demo.tabs": ["화면 예시 선택", "Choose a preview", "選擇預覽模式"],
  "demo.diff": ["파일 비교", "File comparison", "檔案比較"],
  "demo.merge": ["3-way 병합", "Three-way merge", "三方合併"],
  "demo.reset": ["초기화", "Reset", "重設"],
  "demo.region": ["비교·병합 화면 예시. 좁은 화면에서는 좌우로 스크롤할 수 있습니다.", "Comparison and merge preview. Scroll horizontally on narrow screens.", "比較與合併預覽。窄螢幕可左右捲動。"],
  "demo.original": ["원본 코드", "Original code", "原始程式碼"],
  "demo.modified": ["수정본 코드", "Modified code", "修改後的程式碼"],
  "demo.source": ["변경 내용 A", "Changes A", "變更 A"],
  "demo.target": ["변경 내용 B", "Changes B", "變更 B"],
  "demo.result": ["병합 결과", "Merge result", "合併結果"],
  "demo.apply": ["원본을 수정본에 적용", "Apply original to modified", "將原始內容套用至修改版本"],
  "demo.applySource": ["Source 적용", "Apply Source", "套用 Source"],
  "demo.applyTarget": ["Target 적용", "Apply Target", "套用 Target"],
  "demo.hint": ["버튼을 눌러 비교와 병합을 체험해 보세요.", "Try the buttons to compare and merge.", "按下按鈕體驗比較與合併。"],
  "demo.caption": ["앱의 동작을 단순화한 화면 예시입니다.", "A simplified preview of the desktop app.", "此為桌面應用程式的簡化預覽。"],
  "demo.noChanges": ["동일한 파일 · 변경 사항 없음", "Identical files · No changes", "檔案相同 · 無變更"],
  "demo.changes": ["2줄 변경 / 1개 블록", "2 changed lines / 1 block", "2 行變更 / 1 個區塊"],
  "demo.resolved": ["충돌 해결됨 · 결과 적용 완료", "Conflict resolved · Result applied", "衝突已解決 · 已套用結果"],
  "demo.conflict": ["1개 충돌 / Base에서 시작", "1 conflict / Starting from Base", "1 個衝突 / 以 Base 為起點"],
  "demo.commentBase": ["// Base · 충돌을 해결해 주세요", "// Base · Resolve the conflict", "// Base · 請解決衝突"],
  "demo.commentResolved": ["// {side} 적용 · 충돌 해결됨", "// {side} applied · Conflict resolved", "// 已套用 {side} · 衝突已解決"],
  "demo.noscript": ["화면 예시의 전환과 명령어 복사에는 JavaScript가 필요합니다. 다운로드와 사용 안내는 그대로 이용할 수 있습니다.", "Switching previews and copying commands require JavaScript. Downloads and instructions remain available.", "切換預覽及複製指令需要 JavaScript。下載與使用說明仍可使用。"],
  "built.intro": ["만드는 일에 집중할 수 있도록.", "More room to focus on building.", "讓您專注於創作。"],
  "built.diff": ["2-way diff", "2-way diff", "雙檔比較"],
  "built.merge": ["3-way merge", "3-way merge", "三方合併"],
  "built.open": ["Open source", "Open source", "開放原始碼"],
  "built.label": ["기술과 지원 기능", "Technology and capabilities", "技術與支援功能"],
  "features.eyebrow": ["01 / MADE FOR THE DETAILS", "01 / MADE FOR THE DETAILS", "01 / 專注每個細節"],
  "features.title": ["변경의 맥락까지,<br>놓치지 않도록.", "Keep every change<br>in context.", "掌握每處變更，<br>保留完整脈絡。"],
  "features.intro": ["필요한 부분을 또렷하게 보고,<br>익숙한 키보드 흐름으로 작업하세요.", "See what matters clearly,<br>with familiar keyboard shortcuts.", "清楚查看關鍵內容，<br>搭配熟悉的鍵盤快捷鍵工作。"],
  "features.next": ["다음 변경으로 이동", "Next change", "前往下一處變更"],
  "features.compare": ["나란히 비교하고, 바로 편집하고.", "Compare side by side. Edit directly.", "並排比較，直接編輯。"],
  "features.compareBody": ["줄·단어·문자 단위 하이라이트로 달라진 곳을 확인하세요. 오른쪽 파일을 직접 편집하거나 변경 블록을 적용하고, Undo / Redo로 되돌릴 수 있습니다.", "Spot changes with line, word, and character highlighting. Edit the right-hand file directly or apply a change block, then use Undo or Redo when needed.", "以行、單字及字元醒目提示找出差異。直接編輯右側檔案或套用變更區塊，並可使用復原與重做。"],
  "features.sync": ["동기화 스크롤", "Synchronized scrolling", "同步捲動"],
  "features.fold": ["동일한 구간 접기", "Fold unchanged sections", "摺疊相同區段"],
  "features.whitespace": ["공백 비교 옵션", "Whitespace options", "空白字元比較選項"],
  "features.merge": ["세 갈래의 변경을 하나의 결과로.", "Three versions. One result.", "三個版本，合為一個結果。"],
  "features.mergeBody": ["공통 원본을 기준으로 겹치지 않는 변경을 자동 병합합니다. 충돌은 Source와 Target을 비교해 선택하거나, 가운데 Result에서 직접 편집하세요.", "Automatically merge nonoverlapping changes against a common ancestor. Resolve conflicts by choosing Source or Target, or edit the central Result directly.", "以共同原始版本為基準，自動合併不重疊的變更。比較並選擇 Source 或 Target，或直接編輯中央的 Result 來解決衝突。"],
  "features.conflicts": ["충돌 탐색", "Conflict navigation", "瀏覽衝突"],
  "features.save": ["명시적인 결과 저장", "Explicit result saving", "手動儲存結果"],
  "features.workflow": ["지금 쓰는 워크플로에 연결.", "Fits your existing workflow.", "融入現有的工作流程。"],
  "features.workflowBody": ["터미널에서 두 파일을 열거나 P4V의 외부 비교·병합 도구로 등록하세요. 결과 저장과 종료 코드로 다음 작업을 이어갈 수 있습니다.", "Open two files from the terminal or register rdiff as an external diff and merge tool in P4V. Saved results and exit codes connect it to your next step.", "從終端機開啟兩個檔案，或將 rdiff 設為 P4V 的外部比較與合併工具。透過儲存結果與結束代碼銜接下一步工作。"],
  "features.utf8": ["UTF-8 텍스트", "UTF-8 text", "UTF-8 文字"],
  "start.eyebrow": ["02 / FITS YOUR FLOW", "02 / FITS YOUR FLOW", "02 / 配合您的工作方式"],
  "start.title": ["설치하고, 바로 비교하세요.", "Install. Start comparing.", "安裝後，立即開始比較。"],
  "start.guide": ["전체 사용 가이드", "Full user guide", "完整使用指南"],
  "start.label": ["GET STARTED", "GET STARTED", "開始使用"],
  "start.download": ["운영체제에 맞는 버전 받기", "Download for your operating system", "下載適合您作業系統的版本"],
  "start.downloadBody": ["Windows는 <a href=\"https://github.com/zenogrid-law80/rdiff/releases/latest\">최신 릴리스</a>의 MSI 또는 포터블 EXE를 사용하세요. Mac은 <a href=\"https://github.com/zenogrid-law80/rdiff/releases/latest/download/rdiff-macos-arm64.dmg\">DMG</a>를 열고 rdiff를 Applications로 옮기세요.", "On Windows, choose the MSI or portable EXE from the <a href=\"https://github.com/zenogrid-law80/rdiff/releases/latest\">latest release</a>. On Mac, open the <a href=\"https://github.com/zenogrid-law80/rdiff/releases/latest/download/rdiff-macos-arm64.dmg\">DMG</a> and drag rdiff to Applications.", "Windows 請使用<a href=\"https://github.com/zenogrid-law80/rdiff/releases/latest\">最新版本</a>的 MSI 或可攜式 EXE。Mac 請開啟 <a href=\"https://github.com/zenogrid-law80/rdiff/releases/latest/download/rdiff-macos-arm64.dmg\">DMG</a>，並將 rdiff 拖曳至 Applications。"],
  "start.connect": ["비교할 파일 연결하기", "Choose files to compare", "選擇要比較的檔案"],
  "start.connectBody": ["앱의 파일 열기 기능을 사용하거나, 실행 파일 경로와 함께 오른쪽 명령어를 사용하세요.", "Open files in the app, or run the commands shown here using the executable's path.", "使用應用程式的開啟檔案功能，或搭配執行檔路徑執行此處的指令。"],
  "start.review": ["변경을 검토하고 저장하기", "Review and save changes", "檢查並儲存變更"],
  "start.reviewBody": ["필요한 변경을 적용한 후 저장하세요. 병합에서는 모든 충돌을 해결한 뒤 결과를 저장합니다.", "Apply the changes you need and save. When merging, resolve all conflicts before saving the result.", "套用所需的變更後儲存。合併時，請先解決所有衝突，再儲存結果。"],
  "command.tabs": ["사용 환경 선택", "Choose a command environment", "選擇指令環境"],
  "command.terminal": ["터미널", "Terminal", "終端機"],
  "command.p4v": ["P4V 설정", "P4V setup", "P4V 設定"],
  "command.diff": ["두 파일 비교", "Compare two files", "比較兩個檔案"],
  "command.merge": ["3-way 병합", "Three-way merge", "三方合併"],
  "command.diffArgs": ["Diff 도구 인수", "Diff tool arguments", "Diff 工具引數"],
  "command.mergeArgs": ["Merge 도구 인수", "Merge tool arguments", "Merge 工具引數"],
  "command.diffCopy": ["파일 비교 명령어 복사", "Copy file comparison command", "複製檔案比較指令"],
  "command.mergeCopy": ["3-way 병합 명령어 복사", "Copy three-way merge command", "複製三方合併指令"],
  "command.p4vDiffCopy": ["P4V 비교 인수 복사", "Copy P4V diff arguments", "複製 P4V 比較引數"],
  "command.p4vMergeCopy": ["P4V 병합 인수 복사", "Copy P4V merge arguments", "複製 P4V 合併引數"],
  "command.note": ["Windows는 <code>.\\rdiff.exe</code>, macOS는 <code>/Applications/rdiff.app/Contents/MacOS/rdiff</code>를 사용하세요. PATH에 등록하면 위 명령어를 그대로 쓸 수 있습니다.", "Use <code>.\\rdiff.exe</code> on Windows or <code>/Applications/rdiff.app/Contents/MacOS/rdiff</code> on macOS. Add it to PATH to use the commands as shown.", "Windows 使用 <code>.\\rdiff.exe</code>，macOS 使用 <code>/Applications/rdiff.app/Contents/MacOS/rdiff</code>。加入 PATH 後即可直接使用上述指令。"],
  "command.p4vNote": ["P4V의 외부 Diff / Merge 도구에 Windows는 <code>rdiff.exe</code>, macOS는 <code>/Applications/rdiff.app/Contents/MacOS/rdiff</code>의 전체 경로를 지정하세요. 인수 순서는 Source, Target, Base, Result입니다.", "In P4V's external Diff / Merge tools, enter the full path to <code>rdiff.exe</code> on Windows or <code>/Applications/rdiff.app/Contents/MacOS/rdiff</code> on macOS. The argument order is Source, Target, Base, Result.", "在 P4V 的外部 Diff / Merge 工具中，Windows 請指定 <code>rdiff.exe</code> 的完整路徑，macOS 請指定 <code>/Applications/rdiff.app/Contents/MacOS/rdiff</code>。引數順序為 Source、Target、Base、Result。"],
  "copy": ["복사", "Copy", "複製"],
  "copy.done": ["복사 완료", "Copied", "已複製"],
  "copy.manual": ["Ctrl+C로 복사", "Press Ctrl+C to copy", "按 Ctrl+C 複製"],
  "copy.command": ["복사할 명령어", "Command to copy", "要複製的指令"],
  "copy.success": ["{command} 완료", "{command}: done", "{command}：完成"],
  "copy.fallback": ["명령어를 선택했습니다. Ctrl+C를 눌러 복사하세요.", "Command selected. Press Ctrl+C to copy.", "已選取指令。請按 Ctrl+C 複製。"],
  "faq.eyebrow": ["03 / GOOD TO KNOW", "03 / GOOD TO KNOW", "03 / 使用前須知"],
  "faq.title": ["시작 전,<br>궁금한 것들.", "Before you<br>get started.", "開始之前，<br>您可能想知道。"],
  "faq.issue": ["이슈 남기기", "Report an issue", "回報問題"],
  "faq.support": ["어떤 파일과 운영체제를 지원하나요?", "Which files and operating systems are supported?", "支援哪些檔案與作業系統？"],
  "faq.supportBody": ["Windows x64와 macOS 11 이상 Apple Silicon(M1 이후)을 지원합니다. Intel Mac용 빌드는 아직 제공하지 않습니다. 파일당 최대 2 MiB의 UTF-8 텍스트를 지원하며 UTF-8 BOM은 저장 시 유지합니다. 바이너리, UTF-16, 폴더 비교는 지원하지 않습니다.", "Supports Windows x64 and Apple Silicon Macs (M1 or later) running macOS 11 or newer. Intel Mac builds are not available yet. UTF-8 text files up to 2 MiB each are supported, and UTF-8 BOMs are preserved when saving. Binary files, UTF-16, and folder comparison are not supported.", "支援 Windows x64，以及搭載 Apple Silicon（M1 或更新晶片）、執行 macOS 11 或更新版本的 Mac。目前尚未提供 Intel Mac 版本。支援每個檔案最大 2 MiB 的 UTF-8 文字，儲存時會保留 UTF-8 BOM。不支援二進位檔案、UTF-16 及資料夾比較。"],
  "faq.installer": ["MSI와 포터블 EXE는 어떻게 다른가요?", "How do MSI and portable EXE differ?", "MSI 與可攜式 EXE 有何不同？"],
  "faq.installerBody": ["MSI는 현재 사용자 계정에 설치되고 시작 메뉴에 바로가기를 만듭니다. 관리자 권한 없이 설치할 수 있으며 Windows 설치된 앱에서 제거할 수 있습니다. 포터블 EXE는 원하는 폴더에 두고 바로 실행할 수 있습니다.", "The MSI installs for the current user and creates a Start menu shortcut. It needs no administrator privileges and can be removed from Windows Installed apps. Put the portable EXE in any folder and run it directly.", "MSI 會安裝至目前的使用者帳戶，並建立開始功能表捷徑。安裝時不需要系統管理員權限，可從 Windows「已安裝的應用程式」移除。可攜式 EXE 放在任意資料夾即可直接執行。"],
  "faq.mac": ["Mac에서 설치하고 업데이트하려면 어떻게 하나요?", "How do I install and update on Mac?", "如何在 Mac 上安裝與更新？"],
  "faq.macBody": ["DMG를 열어 rdiff.app을 Applications로 드래그한 뒤 실행하세요. 현재 Mac 배포본은 Apple 공증을 받지 않았습니다. 첫 실행이 차단되면 시스템 설정 → 개인정보 보호 및 보안에서 앱을 확인한 뒤 '그래도 열기'를 선택하세요. 자세한 절차는 <a href=\"https://support.apple.com/ko-kr/102445\">Apple의 앱 실행 안내</a>를 참고하세요. Mac 업데이트는 최신 DMG를 받아 앱을 교체하면 됩니다.", "Open the DMG, drag rdiff.app to Applications, then launch it. The current Mac build is not notarized by Apple. If the first launch is blocked, review the app in System Settings → Privacy & Security and choose Open Anyway. See <a href=\"https://support.apple.com/en-us/102445\">Apple's app opening guide</a> for details. To update, download the latest DMG and replace the app.", "開啟 DMG，將 rdiff.app 拖曳至 Applications 後執行。目前 Mac 發行版尚未經 Apple 公證。若首次開啟遭到阻擋，請在「系統設定」→「隱私權與安全性」中確認應用程式，再選擇「強制打開」。詳細步驟請參閱 <a href=\"https://support.apple.com/zh-tw/102445\">Apple 的應用程式開啟說明</a>。更新時，下載最新 DMG 並取代應用程式即可。"],
  "faq.inputs": ["병합하면 원본 파일도 바뀌나요?", "Does merging change the input files?", "合併會更動原始檔案嗎？"],
  "faq.inputsBody": ["3-way 병합에서는 Source, Target, Base를 입력으로 읽고 지정한 Result 경로에만 저장합니다. 자동 병합이 끝나도 저장 버튼을 누르기 전에는 결과 파일을 쓰지 않습니다. Result 경로는 보존할 입력 파일과 다르게 지정하세요.", "Three-way merge reads Source, Target, and Base as inputs and saves only to the specified Result path. Even an automatic merge writes nothing until you choose Save. Use a Result path different from any input you want to preserve.", "三方合併會讀取 Source、Target 與 Base 作為輸入，僅儲存至指定的 Result 路徑。即使自動合併完成，按下儲存之前也不會寫入結果檔案。請將 Result 設為與要保留的輸入檔案不同的路徑。"],
  "faq.updates": ["업데이트와 소스 코드는 어디서 확인하나요?", "Where can I find updates and source code?", "哪裡可以取得更新與原始碼？"],
  "faq.updatesBody": ["<a href=\"https://github.com/zenogrid-law80/rdiff/releases\">GitHub Releases</a>에서 릴리스를 확인할 수 있습니다. Windows에서는 앱의 업데이트 확인 기능이나 <code>rdiff self-update --check</code> 명령도 사용할 수 있습니다. macOS는 최신 DMG로 직접 업데이트하세요. 소스 코드는 <a href=\"https://github.com/zenogrid-law80/rdiff\">GitHub 저장소</a>에서 MIT 라이선스로 공개합니다.", "Find releases on <a href=\"https://github.com/zenogrid-law80/rdiff/releases\">GitHub Releases</a>. On Windows, you can also use the app's update checker or <code>rdiff self-update --check</code>. On macOS, update manually using the latest DMG. The source is available under the MIT license in the <a href=\"https://github.com/zenogrid-law80/rdiff\">GitHub repository</a>.", "您可以在 <a href=\"https://github.com/zenogrid-law80/rdiff/releases\">GitHub Releases</a> 查看發行版本。Windows 也可使用應用程式的更新檢查功能或 <code>rdiff self-update --check</code> 指令。macOS 請使用最新 DMG 手動更新。原始碼以 MIT 授權條款公開於 <a href=\"https://github.com/zenogrid-law80/rdiff\">GitHub 儲存庫</a>。"],
  "download.eyebrow": ["LESS FRICTION. MORE FLOW.", "LESS FRICTION. MORE FLOW.", "減少阻礙，工作更順暢。"],
  "download.title": ["다음 변경부터,<br>rdiff와 함께.", "Your next change,<br>with rdiff.", "下一次變更，<br>就用 rdiff。"],
  "download.intro": ["비교와 병합에 집중할 수 있는 작은 도구.", "A small tool that keeps you focused on comparing and merging.", "讓您專注於比較與合併的輕巧工具。"],
  "footer.credit": ["Built with Rust &amp; GPUI. <span class=\"footer-package\">Package: ridff-tools</span>", "Built with Rust &amp; GPUI. <span class=\"footer-package\">Package: ridff-tools</span>", "以 Rust 與 GPUI 打造。<span class=\"footer-package\">套件：ridff-tools</span>"],
  "footer.releases": ["릴리스", "Releases", "發行版本"],
  "footer.license": ["MIT License", "MIT License", "MIT 授權條款"]
});

window.SiteI18n = (() => {
  const languages = ["ko", "en", "zh-Hant"];
  const themes = ["system", "light", "dark"];
  const root = document.documentElement;
  const media = window.matchMedia("(prefers-color-scheme: dark)");
  const read = (key) => { try { return window.localStorage.getItem(key); } catch { return null; } };
  const write = (key, value) => { try { window.localStorage.setItem(key, value); } catch { /* Private browsing can block storage. */ } };
  const normalize = (value) => {
    const primary = String(value).toLowerCase().split(/[-_]/)[0];
    return primary === "zh" ? "zh-Hant" : languages.includes(primary) ? primary : null;
  };
  const savedLanguage = read("rdiff.website.language");
  let language = languages.includes(savedLanguage) ? savedLanguage
    : (navigator.languages || [navigator.language]).map(normalize).find(Boolean) || "en";
  const savedTheme = read("rdiff.website.theme");
  let theme = themes.includes(savedTheme) ? savedTheme : "system";

  function applyTheme() {
    const resolved = theme === "system" ? media.matches ? "dark" : "light" : theme;
    root.dataset.theme = resolved;
    root.dataset.appearance = theme;
    root.style.colorScheme = resolved;
    document.querySelector('meta[name="theme-color"]')?.setAttribute("content", resolved === "dark" ? "#101313" : "#f7f9f5");
  }
  const onSystemChange = () => { if (theme === "system") applyTheme(); };
  if (media.addEventListener) media.addEventListener("change", onSystemChange);
  else media.addListener(onSystemChange);
  root.lang = language;
  applyTheme();

  return Object.freeze({
    messages: siteMessages,
    get language() { return language; },
    get theme() { return theme; },
    t(key, values = {}) {
      const text = siteMessages[key]?.[languages.indexOf(language)] ?? key;
      return text.replace(/\{(\w+)\}/g, (match, name) => values[name] ?? match);
    },
    setLanguage(value) {
      if (!languages.includes(value)) return;
      language = value;
      root.lang = language;
      write("rdiff.website.language", language);
      window.dispatchEvent(new CustomEvent("rdiff:languagechange"));
    },
    setTheme(value) {
      if (!themes.includes(value)) return;
      theme = value;
      write("rdiff.website.theme", theme);
      applyTheme();
      window.dispatchEvent(new CustomEvent("rdiff:themechange"));
    }
  });
})();
