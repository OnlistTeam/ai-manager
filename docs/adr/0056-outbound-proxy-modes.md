# ADR-0056: The app's own requests follow the system proxy, go direct, or use a custom one

- Status: accepted
- Date: 2026-09-27
- Amends: ADR-0013 decision on the update proxy, ARCHITECTURE §5.13 (the
  proxy installers receive).

## Context

Settings had a "local download proxy": a switch and an address field. With
the switch off, requests did not go direct. The inherited HTTP client follows
the `*_PROXY` variables and then the system's proxy settings, so every request
still went through whatever the system named. There was no way to go direct.

That mattered as soon as the model picker read a relay's catalogue. The same
292 KB list took 0.6 s direct and 8 to 16 s through a local proxy the system
pointed at, and a user whose relay is reachable directly could not skip the
proxy. The switch was also named for downloads, while the client serves every
request the app itself sends.

## Decision

1. **One setting with three modes.** "Follow system" (the default and what
   the app did before), "Direct", and "Custom". The row states in one line
   what requests use now: the custom address, the proxy in `HTTPS_PROXY`, the
   system's proxy, "the system names none", or "direct".
2. **Scope.** The setting covers requests the app itself sends: model lists
   and tests, speed tests, downloads, updates, installers it starts, and
   requests it forwards while routing. A tool that talks to its endpoint
   directly is not affected; its own environment decides.
3. **Storage.** A custom address stays in the inherited `global_proxy_url`
   setting, with the existing rule: a loopback address without credentials.
   "Direct" is a product setting, `network_proxy_direct`, beside it. A saved
   custom address wins over it. At startup the flag is read before the
   global client is first built.
4. **Direct means direct everywhere.** The global client is built with no
   proxy. The updater, which builds its own client, is told the same.
   Installers get blank `*_PROXY` variables, which curl, npm and the other
   installers read as unset, so a proxy the app inherited is not used either.
5. **Follow system reaches installers too.** An app opened from the Dock or
   the Start menu has no `*_PROXY` variables, and installers read nothing
   else. While following the system, an installer is given the system's
   proxy when the environment names none and the proxy needs no
   credentials. A custom address is handed on as before.
6. **Reading the system's proxy.** The line in Settings needs the address
   the client will use. It comes from the matcher in `hyper-util`, which the
   HTTP client itself uses: the environment first, then the operating system
   (macOS network settings, Windows Internet Options). `hyper-util` was
   already a direct dependency; this enables its `client-proxy` and
   `client-proxy-system` features, which `reqwest` already turns on, so no
   new crate is compiled. The address is shown as `scheme://host:port`,
   never with credentials.

## Consequences

- A user can skip a slow local proxy for everything the app sends, with one
  click, and see which proxy is in use without opening system settings.
- Following the system now also covers installers started from the Dock,
  which used to go direct and fail where only a proxy gets through.
- A SOCKS address such as `socks5://127.0.0.1:1080` is now accepted: its
  host arrives as a domain string, which the loopback check did not parse
  before.
