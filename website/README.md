# aimanager.tools

The product download page. It exists because the application hard-codes
`https://aimanager.tools/download` as the manual fallback a user reaches when
an automatic update fails, so that address has to resolve to something useful.

## What it is

Five static files and no build step. `download.js` reads the public GitHub
release list at request time and renders whatever is newest, which means
publishing a release never requires redeploying this site. When GitHub cannot
be reached the page falls back to a plain link to the releases page.

| File | Purpose |
| --- | --- |
| `index.html` | The whole page |
| `styles.css` | The application's palette, reused |
| `download.js` | Release lookup, platform detection, rendering |
| `icon.png` | The application icon, copied from `src-tauri/icons/` |
| `_redirects` | `/download` to `/`, because the app opens that exact path |
| `_headers` | CSP and transport headers |

The content security policy allows exactly one outbound connection,
`https://api.github.com`. Adding an analytics script or a web font would
require widening it, which is the point.

## Deploying

Cloudflare Pages project `aimanager-site`, serving `aimanager.tools` and
`www.aimanager.tools` from this directory. The zone already hosts
`dl.aimanager.tools`, the R2 update channel.

```bash
npx wrangler pages deploy website --project-name=aimanager-site
```

The API token lives in the macOS keychain and is read at deploy time only.
It is never written to this repository, a log, or a workflow file.

## Editing

Keep the claims true. The page states that macOS builds are notarized, that
Windows builds carry no Authenticode signature, and that Linux has no platform
signature at all. If any of that changes, this page changes with it, along
with `README.md` and `SECURITY.md`.
