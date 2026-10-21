# rdiff website

Static Korean, English, and Traditional Chinese landing page for <https://rdiff.zenogrid.co.kr>.

## Preview

Open `index.html` directly, or run this command from the repository root:

```powershell
python -m http.server 4173 --bind 127.0.0.1 --directory website
```

Then open <http://127.0.0.1:4173>. No package installation or build step is required.

## Hosting

Serve the contents of `website/` as the site's document root. Keep `index.html`, `styles.css`, `i18n.js`, `script.js`, and `favicon.svg` together. Assets use relative paths, so the page also works under a subdirectory.

The browser favicon, header and footer logos, and interactive preview app icon all use `favicon.svg`. Update this single file to change the website's product icon.

The canonical URL and Open Graph URL in `index.html` point to `https://rdiff.zenogrid.co.kr/`. Update both if the public domain changes.

### GitHub Pages

`.github/workflows/pages.yml` deploys this directory when changes to `website/` or the workflow are pushed to `main`. It can also be run manually through GitHub Actions. The workflow checks JavaScript syntax, runs preference and translation tests, and publishes the static files without a build step.

The repository's Pages publishing source is **GitHub Actions**, with custom domain `rdiff.zenogrid.co.kr`. Its DNS CNAME points to `zenogrid-law80.github.io`. With an Actions deployment, the custom domain is stored in the repository's Pages settings; a `CNAME` file is not needed.

## Content

- Installer buttons open GitHub's latest release, where the versioned MSI can be selected.
- The portable Windows download uses the stable release asset name `rdiff-windows-x86_64.exe`.
- Both macOS buttons download `rdiff-macos-arm64.dmg` from the latest release. It supports macOS 11 or later on Apple Silicon; Intel Mac builds are not provided.
- Mac installation, the app-bundle CLI/P4V path, lack of Apple notarization, and manual DMG updates are explained on the page.
- The interactive diff and merge previews use fixed examples; they do not read or upload files.
- Command examples and P4V argument ordering follow the repository's CLI and README.
- The product and executable are `rdiff`; the Cargo package is `ridff-tools`.
- Fonts, icons, scripts, and styles have no external runtime dependencies.

## Checks

```powershell
node --check website/i18n.js
node --check website/script.js
node --test tests/website.test.cjs
git diff --check -- website
```

Check desktop and mobile widths in all three languages and themes, both demo tabs, apply/reset actions, command tabs and copy buttons, keyboard navigation, and FAQ expansion when editing the page. Also check persistence after reloading and live system theme changes.

## Language and appearance

- The header selects 한국어 (`ko`), English (`en`), or 繁體中文 (`zh-Hant`), plus System, Light, or Dark appearance.
- A first visit uses the first supported browser language, falling back to English. Chinese locales use Traditional Chinese. System appearance follows operating system changes live.
- Language and appearance are saved independently in `localStorage` as `rdiff.website.language` and `rdiff.website.theme`. Blocked storage still allows changes for the current visit.
- `i18n.js` applies the stored theme before CSS loads and contains all translations, including macOS installation notes. `script.js` translates the page, metadata, accessibility labels, demo status, and copy feedback without resetting demo choices. Code examples and CLI paths remain unchanged.
