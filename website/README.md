# aimanager.tools

The product download page. It exists because the application hard-codes
`https://aimanager.tools/download` as the manual fallback a user reaches when
an automatic update fails, so that address has to resolve to something useful.

## What it is

Seven static files and no build step. `download.js` reads a small manifest the
release workflow publishes to the project's own distribution host, so
publishing a release never requires redeploying this site.

| File          | Purpose                                                   |
| ------------- | --------------------------------------------------------- |
| `index.html`  | The whole page, with its English copy                     |
| `i18n.js`     | The four languages and how one is chosen                  |
| `styles.css`  | Paper-and-ink palette taken from the product icon         |
| `download.js` | Manifest lookup, platform detection, rendering            |
| `icon.png`    | The application icon, copied from `src-tauri/icons/`      |
| `_redirects`  | `/download` to `/`, because the app opens that exact path |
| `_headers`    | CSP and transport headers                                 |

The content security policy allows exactly one outbound connection,
`https://dl.aimanager.tools`. Adding an analytics script or a web font would
require widening it, which is the point.

## Where the downloads come from

`sync-r2.yml` already mirrors every release asset to
`https://dl.aimanager.tools/ai-manager/<tag>/`. This page reads
`download.json` from the root of that prefix, which
`scripts/build-download-manifest.mjs` builds during the same workflow run:
version, publication date, and for each of the five installers its name, size,
SHA-256 and URL.

GitHub is not on the page's critical path. The installers are already on our
host, so asking the GitHub API for their sizes would put the slowest hop in
front of every visitor, and that hop is slowest exactly where most readers are.
The unauthenticated API also allows only sixty calls an hour per address, which
a shared office or a campus can exhaust without anyone noticing. GitHub stays as
the stated alternative: a line under the table, and the page's whole answer when
the manifest cannot be read.

Stable is tried first and `staging/download.json` second, so a prerelease is
only offered while no stable release exists. When the page falls back to
staging it says so.

Two pieces of Cloudflare configuration this depends on:

- the R2 bucket `aimanager-releases` must allow cross-origin `GET` from
  `https://aimanager.tools` and `https://www.aimanager.tools`, or the browser
  blocks the manifest read
- `dl.aimanager.tools` must stay bound to that bucket

## Languages

The page is served in English, Simplified Chinese, Traditional Chinese and
Japanese, matching the four the application itself ships. It is one document
rather than one per language, so `/download` keeps resolving no matter which
language the visitor reads.

The language is picked in this order, and remembered in `localStorage` once the
visitor chooses for themselves:

1. `?lang=en|zh|zh-TW|ja`, so a link can point at one language
2. A choice made on an earlier visit
3. `navigator.languages`, with Traditional Chinese tested before Chinese
4. English

English is not in the dictionary twice. The static copy lives in `index.html`
so the page still reads without JavaScript, and `i18n.js` captures it on load
as the fallback for every language; the short `en` block in `i18n.js` holds
only the strings `download.js` builds at run time.

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

A change to the English copy in `index.html` is only half a change: the same
sentence exists in three translations in `i18n.js`, keyed by the element's
`data-i18n`. An untranslated key silently falls back to English, so a stale
translation shows as English rather than as an error.
