# ADR-0030: Encrypted Portable Archive Uses a Versioned Argon2id + XChaCha20-Poly1305 Container

- Status: Proposed — awaiting approval of the cryptographic dependencies and "whether to carry Provider keys"
- Date: 2026-08-29

## Context

R5.8's typed custom Provider is complete, but the existing whole-database import/export is still SQL with `0600`
permissions and a plaintext-key warning. The AES API exposed by `zip 2.4.x` itself explicitly warns that it has
not been adequately reviewed for correctness and cannot be wrapped as a secure archive. Once SecretStore lands,
ordinary database backups should also store only opaque secret references; if a cross-device portable file is to
carry Provider keys, it must have an independent, upgradable format that rejects tampering.

## Proposed Decision

1. Add a v1 binary container with the independent extension `.aimgr-backup` and the ASCII `AIMGRBAK` 8-byte
   magic. It does not masquerade as ZIP and is not registered as a generic compressed archive. The fixed
   little-endian header contains only format/KDF/AEAD parameters, a 16-byte random salt, a 16-byte nonce prefix,
   chunk size, and the lengths required for limits; tools, Providers, user names, paths, and device names all stay
   inside the ciphertext.
2. The password derives a 256-bit key with Argon2id v1.3. v1 generation parameters are fixed to RFC 9106's
   low-memory recommended profile: 64 MiB, 3 iterations, 4 lanes, 128-bit salt, 256-bit output; the parameters are
   written into the header, allowing future versions to raise the cost. Import rejects out-of-range parameters
   before derivation to prevent a malicious file from triggering unbounded memory/CPU.
3. Data uses chunked XChaCha20-Poly1305 AEAD. v1 fixes a 1 MiB plaintext chunk; each nonce is the random 128-bit
   prefix plus a monotonic 64-bit chunk index. The exact header bytes, record type, index, plaintext length, and
   final flag all enter the AAD. The last mandatory authenticated record stores the total chunk/plaintext length
   and the manifest digest; a missing, duplicate, out-of-order, or truncated record, or any single-bit change of
   tag/header, fails before any data is applied.
4. v1 logical entries allow only reviewed names: database SQL, the versioned manifest, and the Provider secret
   bundle when the user explicitly chooses it; file-system paths, symlinks, arbitrary JSON file trees, or unknown
   entry types are not accepted. Ordinary backups never carry keys; "include Provider keys" is off by default,
   requires the password to be entered a second time and a separate confirmation, and never includes OS
   SecretStore references, cloud credentials, logs, sessions, or caches.
5. Reuse the existing in-memory SQL export/import and transaction/rollback paths; v1 produces no plaintext ZIP or
   plaintext temporary files. The password, derived key, KDF workspace, decrypted SQL, and secret bundle are
   placed into a `Zeroizing`/explicit `Zeroize` owner as soon as they enter Rust; there is no "remember archive
   password", and nothing is written to SQLite, SecretStore, logs, Operation output, or the renderer read model.
6. Argon2 uses a caller-allocated `Vec<Block>` and `hash_password_into_with_memory`, explicitly zeroizing the
   whole workspace afterwards. Calling only the crate's alloc helper is not enough: the `zeroize` feature of the
   current 0.6.0 wipes some intermediate values, but the blocks held by the helper are not automatically zeroed
   block by block before deallocation.
7. Before reading the payload, v1 unconditionally rejects: header larger than 4 KiB, manifest larger than 1 MiB,
   more than 8 entries, total plaintext over 256 MiB, chunk size not equal to 1 MiB, KDF memory outside
   19 MiB..256 MiB, iterations outside 1..10, lanes outside 1..8. Wrong password and corrupted ciphertext use the
   same stable error for the UI, without revealing which part of verification failed.
8. Export writes a `.partial` with mode `0600` in the same directory and atomically replaces only after all
   authenticated records are complete and flush/fsync is done; on failure the partial is cleaned up precisely,
   and an existing valid archive is never overwritten. Import first fully authenticates and validates the
   manifest/SQL, then reuses the existing "safe backup of the current database → transactional import → live
   sync → rollback on failure"; network/cloud sync must not bypass this path.

## Exact Dependency Approval

Recommended exact pins in `Cargo.toml`:

```toml
argon2 = { version = "=0.6.0", default-features = false, features = ["zeroize"] }
chacha20poly1305 = { version = "=0.11.0", default-features = false, features = ["getrandom", "zeroize"] }
zeroize = "=1.9.0"
```

- Necessity: the repository's existing `sha2`/`hmac` provide neither a memory-hard password KDF nor
  confidentiality; ZIP AES cannot pass the security bar.
- License/MSRV: all three and the complete transitive graph added/upgraded this time are `MIT OR Apache-2.0`,
  with MSRV no higher than the repository's pinned Rust 1.85; an independent feature-graph
  `cargo check --locked` on the local Rust 1.95 has passed.
- Supply-chain delta: resolving against a temporary copy of the current lockfile yields 18 new crates and upgrades
  `typenum` 1.19.0→1.20.1, `zeroize` 1.8.2→1.9.0, `zeroize_derive` 1.4.3→1.5.0. The 21 new/upgraded archives
  total about 646 KiB compressed and 4.15 MiB unpacked; net of the replaced old versions, about
  539 KiB/3.32 MiB. None of them has a `build.rs`; the existing `getrandom 0.4.3` is reused, and no new C/C++,
  system libraries, runtime services, or download scripts are added.
- Security: the RustCrypto ChaCha20-Poly1305 implementation states it was audited by NCC Group with no
  significant issues; the XChaCha 192-bit nonce suits this product's random-prefix+counter scenario, which does
  not require cross-library interoperability. Queries on 2026-08-29 against GitHub Advisory exact-version and the
  RustSec advisory-db crate path returned no hits, but this is only a check of currently known advisories and
  does not replace format tests and future audits.
- Maintenance: all three are maintained under RustCrypto team ownership. `argon2 0.6.0` was released on
  2026-08-27; its advantage is that it adds allocation-failure returns and parameter validation, its risk is that
  it is only two days old; hence the exact pin, the retained RFC/KAT/tamper corpus, and a separate review of the
  changelog and lock diff for any upgrade. `chacha20poly1305 0.11.0` and `zeroize 1.9.0` were released about two
  months ago.
- `secrecy` not chosen: at this stage the in-house redacted domain newtype + `zeroize::Zeroizing` already covers
  the Debug/Drop boundary; introducing another wrapper cannot wipe the copies Tauri/serde produce before entering
  the Rust owner and would only widen the dependency surface.

## Acceptance Gates

Cover at least: RFC 9106 Argon2id vectors, RustCrypto/public XChaCha vectors, fixed-format golden files, cross
macOS/Windows/Linux decryption, wrong password, header/body/tag bit flips, truncation/appending, record
reordering/duplication, nonce uniqueness, KDF/size limits, partial write failure, current-database rollback, no
keys in logs/Operation/IPC, and, when SecretStore is missing, "key-free ordinary backup still usable, key-carrying
portable archive fails closed". Until this evidence is complete, the product continues to call the existing SQL a
plaintext export only.

## Pending Owner Decision

1. Whether to approve the three exact Cargo dependencies above and the format boundary;
2. Whether to accept v1's default KDF cost of 64 MiB/3/4, and to allow adjustment only through a new format
   version after real-world testing on the oldest supported device;
3. Whether to allow the off-by-default explicit option "include Provider keys". If not allowed, v1 can still
   encrypt the database and settings but does not resolve SecretStore values;
4. Whether to accept the 256 MiB v1 total plaintext limit. When exceeded, only prompt to clean up usage/history
   first; never silently split the archive or downgrade encryption.

## References

- RFC 9106 Argon2: <https://www.rfc-editor.org/rfc/rfc9106.html>
- OWASP Password Storage Cheat Sheet:
  <https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html>
- RustCrypto Argon2: <https://github.com/RustCrypto/password-hashes/tree/master/argon2>
- RustCrypto ChaCha20-Poly1305:
  <https://github.com/RustCrypto/AEADs/tree/master/chacha20poly1305>
- libsodium XChaCha20-Poly1305:
  <https://doc.libsodium.org/secret-key_cryptography/aead/chacha20-poly1305/xchacha20-poly1305_construction>
