# ADR 0039: Provider selection evidence

## Decision

Keep CC Switch's additive provider semantics. Adding an OpenCode provider does
not select a default model. A database selection is not runtime evidence.

Report explicit global model selection, recent model history, and unknown
selection separately. Read OpenCode's local model history and credential store
without writing either file or returning credential contents. Recent history is
not proof of a running session; project configuration and command-line flags
may override the global snapshot. Never guess that the only saved provider is
active. Unknown built-in model catalogs must not be silently replaced with an
older known custom provider.

Configuration read/parse failures must not become an official/default-provider
match. The UI must distinguish missing evidence from an unused saved entry.
Only credential presence and its source may cross the new runtime boundary.
Successful activation/failover and uncertain write failures invalidate the runtime
query, and failed refreshes do not promote stale cached evidence. Provider IDs alone
cannot match a stale saved OpenCode endpoint with a different live URL; URL paths
remain case-sensitive. The native additive capability also controls action copy.

## Verification

Presentation clarification (2026-09-20, user-directed): saved provider API keys
remain visible and copyable on cards and in edit inputs. Runtime credential
evidence is only a fallback when no saved key exists; it never replaces the saved
value. The runtime model still carries no secret and does not imply that a saved
key is the credential currently in use. URLs are also copyable in cards and forms.

Use synthetic files and isolated homes for native regression tests; cover
explicit model priority, recent selection, disabled providers, credential-store
presence, malformed files, and honest frontend state/labels. Never alter the
user's actual tool configuration as part of verification.
