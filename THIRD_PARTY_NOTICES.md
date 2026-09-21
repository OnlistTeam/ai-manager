# Third-Party Notices

AI Manager as a whole is distributed under the GNU Affero General Public
License, version 3 or later (see [LICENSE](./LICENSE)). It is built on the
following open-source work, whose own licenses and copyright notices are
preserved in full.

## CC Switch

- Repository: https://github.com/farion1231/cc-switch
- Copyright (c) 2025 Jason Young
- License: MIT License (see [LICENSE-MIT](./licenses/LICENSE-MIT))

AI Manager is a downstream fork of CC Switch, forked at v3.19.2. The MIT
License permits sublicensing, so the parts derived from CC Switch are
redistributed here as part of an AGPL-licensed combined work. This does not
change their upstream terms: the same code remains available under MIT from
the upstream project, and anyone may obtain it there under those terms.

The original MIT License text and copyright notice are retained in
[LICENSE-MIT](./licenses/LICENSE-MIT), and existing copyright headers in upstream source
files must not be removed. Upstream code is reused through the
`src-tauri/src/compat/ccswitch/` compatibility layer and kept in sync by
cherry-picking; see [ADR-0001](docs/adr/0001-upstream-strategy.md).

Because AGPL terms cannot be applied back to an MIT project, changes made in
this repository cannot be contributed upstream to CC Switch as-is.

The AI Manager product name, icons, branding, and user interface are original
to this project and are not part of the upstream work.

## Other dependencies

Runtime dependencies are declared in `package.json` and `src-tauri/Cargo.toml`
and are distributed under their own licenses; the authoritative text for each
one ships with that package and is available from its registry. The set below
was taken from the dependency graph actually linked into the four shipped
targets (`aarch64-apple-darwin`, `x86_64-apple-darwin`,
`x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu`), excluding development
and build-only packages.

Most of the graph is plain permissive: MIT, Apache-2.0, BSD-2-Clause,
BSD-3-Clause, ISC, 0BSD, Zlib, Unlicense, BSL-1.0, MIT-0, and CC0-1.0, several
of them offered as a choice of terms. Three groups are worth naming explicitly,
because "permissive" alone does not describe them:

- **Unicode-3.0** — the ICU crates (`icu_*`, `zerovec`, `yoke`, `tinystr`, and
  the rest of that family, plus `unicode-ident`). Permissive, with an explicit
  requirement to carry the Unicode license notice.
- **MPL-2.0** — `cssparser`, `cssparser-macros`, `dtoa-short`, `option-ext`,
  and `selectors`, reached by way of the webview stack. MPL-2.0 is file-level
  copyleft: it obliges whoever modifies a covered file to offer that file's
  source. None of these files are modified here, and none of them invoke
  Exhibit B ("Incompatible With Secondary Licenses"), so MPL-2.0 section 3.3
  permits distributing them as part of this AGPL-licensed work.
- **CDLA-Permissive-2.0** (`webpki-roots`) and the **OpenSSL** terms that apply
  to part of `aws-lc-sys` alongside its ISC and Apache-2.0 grants. Both are
  permissive and require the notice to be carried.

Every license in the graph permits redistribution inside an AGPL-3.0 combined
work. `LICENSE`, `LICENSE-MIT`, and this file are installed alongside the
application on macOS and Linux so the terms travel with the binary.

The Windows installer does not carry them. It installs per user, and the MSI
validator rejects a per-user component whose key path is a file rather than a
registry value (ICE38), which is what bundling these three files produces.
Until that is solved the Windows terms are reachable from the application's
About screen and from this repository.
