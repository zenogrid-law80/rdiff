# rdiff website

Static Korean landing page for <https://rdiff.zenogrid.co.kr>.

## Preview

Open `index.html` directly, or run this command from the repository root:

```powershell
python -m http.server 4173 --bind 127.0.0.1 --directory website
```

Then open <http://127.0.0.1:4173>. No package installation or build step is required.

## Hosting

Serve the contents of `website/` as the site's document root. Keep `index.html`, `styles.css`, `script.js`, and `favicon.svg` together. Assets use relative paths, so the page also works under a subdirectory.

The canonical URL and Open Graph URL in `index.html` point to `https://rdiff.zenogrid.co.kr/`. Update both if the public domain changes.

### GitHub Pages

`.github/workflows/pages.yml` deploys this directory when changes to `website/` or the workflow are pushed to `main`. It can also be run manually through GitHub Actions. The workflow checks JavaScript syntax and publishes the static files without a build step.

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
node --check website/script.js
git diff --check -- website
```

Check desktop and mobile widths, both demo tabs, apply/reset actions, command tabs and copy buttons, keyboard navigation, and FAQ expansion when editing the page.
