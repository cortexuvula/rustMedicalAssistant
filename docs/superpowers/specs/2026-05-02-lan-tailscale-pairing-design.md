# LAN / Tailscale Pairing — Design

**Date:** 2026-05-02
**Status:** Approved (ready for implementation plan)

## Problem

Today's "Running Across Machines" flow (README.md:104–144) assumes the user can clone and `make` whisper.cpp from source, set `OLLAMA_HOST=0.0.0.0:11434` as an environment variable, know what a hostname or port is, and remember to redo the env-var trick after every reboot. That puts office-server setup out of reach for a clinic without an IT person on retainer — exactly the persona we're trying to support.

The target deployment is a clinic with **one heavy box** that runs Ollama, LM Studio, and (optionally) whisper.cpp as servers and *also* runs FerriScribe locally for one clinician, plus **several client-only machines** belonging to other clinicians who connect to the heavy box over the clinic LAN or a Tailscale tailnet.

Goal: a non-technical clinician can install FerriScribe on the heavy box, click through one wizard, and produce a QR code; every other clinician installs FerriScribe, scans the QR (or chooses the server from a discovered-on-LAN list), and is done. No terminals, no env vars, no source builds.

## Goals

- A "Become office server" wizard that handles persistent Ollama service, whisper.cpp server install/lifecycle, LM Studio detection, firewall, and pairing UI on macOS / Linux / Windows.
- mDNS-based zero-config discovery on the LAN; QR / 6-digit code pairing as a universal fallback that also covers Tailscale and remote clinicians.
- Per-client tokens (not a single shared secret), revocable from a "Connected clients" panel on the server.
- Auth proxy in front of Ollama (Ollama has no native auth) so the headline `OLLAMA_HOST=0.0.0.0` security gap closes.
- Clients see the heavy box's installed models in their existing model pickers — no per-client model downloads.
- The local FerriScribe on the heavy box uses `127.0.0.1` automatically when sharing is on; it does not pair with itself.
- Existing manual hostname/port/API-key configurations keep working; the new flow is additive.

## Non-goals

- A FerriScribe-branded server product. The servers remain Ollama, LM Studio, and whisper.cpp. We launch and proxy them; we do not replace them.
- An auth proxy for LM Studio. LM Studio remote = trust the network (Tailscale recommended). Documented, not solved.
- Multi-server load balancing or HA failover between heavy boxes. One server per clinic.
- Tailscale install or onboarding. We detect and consume an existing tailnet; we do not manage it.
- Mobile clients.
- A FerriScribe Cloud / hosted offering — the local-only constraint stays.
- Migration of existing manual configs into the new flow. They coexist; users can re-pair if they want.

## Decisions

| # | Decision |
|---|---|
| Q1 | Persona: clinic with one shared heavy box, multiple clinician clients. |
| Q2 | Server software: existing external tools (Ollama, LM Studio, whisper.cpp) — no FerriScribe-branded server. |
| Q3 | Discovery: mDNS for same-LAN happy path, QR / 6-digit code for Tailscale and fallback. |
| Q4 | Server-side enablement: in-app "Become office server" wizard. Persistent service via launchd / systemd / scheduled-task. whisper.cpp prebuilt binary downloaded on demand. LM Studio detected and opened, not driven. |
| Q5 | Auth: per-client tokens, traded one-shot enrollment code → long-lived token. Revocable. Tokens stored in the OS keychain on the client. |
| Q6 | Auth-proxy required in front of Ollama (Ollama has no native auth). whisper.cpp `--api-key` is a single shared value, so we shim it the same way: external clients hit the auth-proxy with their per-client token; the proxy forwards to whisper.cpp with the shared key. |
| Q7 | LM Studio stays unproxied. Trust-the-network model documented. |
| Q8 | Heavy box's local FerriScribe uses `127.0.0.1` and bypasses pairing entirely when sharing is on. |
| Q9 | Connection fallback order: LAN address first (500ms connect timeout), Tailscale address second, both stored from the QR. |

## Architecture

### Components added

**On the heavy box (in-process or co-process with FerriScribe):**

1. **Auth proxy** (in-process, `axum` + `reqwest`) on `:11435`
   - Reverse proxies to Ollama on `127.0.0.1:11434`.
   - Validates `Authorization: Bearer <token>` against the token store on every request.
   - Same proxy serves a separate route for whisper.cpp on a second port (e.g. `:8081`) that forwards to whisper.cpp on `127.0.0.1:8080` using whisper.cpp's `--api-key` shared value internally.
2. **mDNS advertiser**
   - Service type: `_ferriscribe._tcp.local.`
   - TXT records: `host=<friendly-name>`, `ollama=11435`, `whisper=8081`, `lmstudio=1234` (omitted if not running), `version=<app-version>`.
3. **Pairing service** on `:11436` (bound to `0.0.0.0` so clients can reach it during enroll; revoke / list endpoints are gated to loopback only)
   - `POST /pair/enroll` — accepts a 6-digit one-shot enrollment code (10-min TTL) plus a label (e.g. "Dr. Smith's MacBook"), returns a long-lived per-client token. Reachable from any pairing client.
   - `POST /pair/revoke` — loopback-only; called from the Settings UI on the heavy box.
   - `GET /pair/clients` — loopback-only; lists labels + last-seen timestamps for the Connected Clients panel.
4. **Token store**
   - Small SQLite file in app data, encrypted with the same SQLCipher key already in the OS keychain (no new secret to manage).
   - Schema: `id`, `label`, `token_hash`, `created_at`, `last_seen_at`, `revoked_at`.
   - Tokens stored as hash-only on the server; raw token returned exactly once at enroll time.
5. **whisper.cpp child process**
   - Prebuilt server binary downloaded on first wizard run from the upstream `ggerganov/whisper.cpp` GitHub Releases into `<app-data>/bin/whisper-server`. SHA256 verified against a manifest pinned in our repo.
   - Reuses the whisper model file the FerriScribe client side already manages (`<app-data>/models/whisper/ggml-large-v3-turbo.bin`).
   - Spawned as a child of FerriScribe with a stdout/stderr supervisor that restarts on crash and stops cleanly when sharing is disabled.

**On every client (including the heavy box's local FerriScribe when sharing is off):**

1. **mDNS browser** — populates the Settings → Sharing screen with discovered servers.
2. **Pairing client** — handles the enrollment-code exchange, stores the issued token in the OS keychain (`keyring` crate, same approach used for the SQLCipher key).
3. **Connection resolver** — for paired connections, tries the LAN address first with a 500ms connect timeout, falls back to the Tailscale address if present, surfaces an "unreachable" state if both fail.
4. **Status badge** — small indicator in the existing app status bar: green/LAN, green/Tailscale, amber/reconnecting, red/unreachable.

### Components changed

- **Ollama provider** (`crates/ai-providers`): adds an `Authorization: Bearer` header when the active connection is a paired one. Localhost paths unchanged.
- **STT remote provider** (`crates/stt-providers`): same — adds bearer header for paired connections.
- **Settings UI** (`src/lib/components/settings/Models.svelte` and friends): adds a Sharing pane on both server and client sides. Reuses existing model-picker components but populates them from the live remote `/api/tags` and `/v1/models` endpoints when paired.

### What stays exactly as-is

- Ollama itself stays bound to `127.0.0.1:11434`. The auth proxy is the only thing exposed.
- LM Studio's existing local-server toggle is the only LM Studio change. We open the LM Studio app for the user from the wizard; we do not drive its CLI.
- Diarization, audio capture, SQLite, RAG, the editors — all unchanged. The wire still carries only Whisper inference and Ollama chat / embedding calls.

## Data flow

**Pairing (one-time per client):**

```
Client (Settings → Sharing)              Heavy box (sharing service)
─────────────────────────────────        ────────────────────────────
mDNS browse → finds server   ←── advertise _ferriscribe._tcp.local.
User clicks Connect, prompted for code
POST /pair/enroll {code, label}  ──→
                                  ←──    {token, ports, lan_addr,
                                          tailscale_addr?}
Store token in OS keychain
```

QR path is identical except mDNS is replaced by scanning a QR that already encodes the server addresses + the enrollment code, so the user only sees a single "Pair" prompt.

**Steady-state STT call:**

```
Client                                    Heavy box
──────                                    ─────────
POST /v1/audio/transcriptions   ──→      :8081 (auth proxy)
  Authorization: Bearer <token>          validates token, swaps in
                                         whisper.cpp's shared --api-key,
                                         forwards to 127.0.0.1:8080
                                  ←──    transcript
```

Ollama calls follow the same shape against `:11435`.

## Server wizard — step-by-step UX

**Settings → Sharing → "Become office server"** (button absent if sharing already enabled; replaced with status panel).

1. **Friendly name** — prefilled from `hostname`, editable. Saved to settings, used in mDNS TXT and the QR.
2. **Ollama persistent service** — wizard detects current Ollama state. If installed but not running as a persistent service, button: "Set up persistent Ollama."
   - macOS: `~/Library/LaunchAgents/com.ferriscribe.ollama.plist`, `launchctl load`.
   - Linux: `~/.config/systemd/user/ollama.service`, `systemctl --user enable --now ollama`.
   - Windows: scheduled task at user logon (`schtasks /create`).
   - All three set `OLLAMA_HOST=127.0.0.1:11434` so Ollama remains localhost-only behind the auth proxy.
3. **Auth proxy** — starts in-process. Master per-client-token-store key generated on first run, stored alongside the SQLCipher master key in the OS keychain. Surfaces as a green check; no user action.
4. **whisper.cpp server**
   - Detects platform → downloads matching prebuilt binary from a pinned GitHub Releases manifest into `<app-data>/bin/whisper-server`.
   - SHA256 verified against a manifest checked into this repo.
   - Reuses `<app-data>/models/whisper/ggml-large-v3-turbo.bin` if present; otherwise prompts to download (existing model-fetch flow).
   - Spawned as child process: `whisper-server --host 127.0.0.1 --port 8080 --api-key <internal-shared-key> -m <model>`. The auth proxy fronts it on `:8081` so whisper.cpp itself stays bound to loopback, mirroring the Ollama treatment.
   - Lifecycle: started when sharing is on, stopped when off, supervisor restarts on unexpected exit (capped backoff), logs to a rotating file in `<app-data>/logs/`.
5. **LM Studio (optional)**
   - Detect by checking the platform-specific install paths.
   - Found: button "Open LM Studio's Local Server tab." We `open` / `xdg-open` / `start` the app and drop a one-line note "Click Start Server in LM Studio's Local Server tab. Default port 1234."
   - Not found: link to download.
6. **Firewall**
   - macOS: triggered by binding the listener; user clicks "Allow" on the system prompt.
   - Windows: try silent `netsh advfirewall firewall add rule …`; if the elevation prompt is denied, fall back to a clear instruction with a Copy button.
   - Linux: best-effort `ufw status` detection with a copy-paste command if active.
7. **Pairing screen**
   - Big QR + 6-digit code text fallback.
   - Code regenerates every 10 minutes or after one successful use.
   - Below the QR: "Connected clients" list with `name · last seen · [Revoke]` rows. Empty initially.

**Subsequent visits to Settings → Sharing on the server** show a compact status panel: each subsystem (Ollama, whisper.cpp, LM Studio, mDNS, auth proxy, pairing) with green/amber/red, "Show pairing QR" button, "Connected clients" list, "Stop sharing" button.

## Client pairing UX

**Settings → Sharing**:

```
┌────────────────────────────────────────────┐
│  Pair with an office server                │
│                                            │
│  Found on your network:                    │
│  ┌────────────────────────────────────┐    │
│  │  Clinic Server                     │    │
│  │  192.168.1.42 · Ollama · Whisper   │    │
│  │                       [Connect]    │    │
│  └────────────────────────────────────┘    │
│                                            │
│  No server here?  [Pair with QR or code]   │
└────────────────────────────────────────────┘
```

Clicking **Connect** asks for the 6-digit code from the server's pairing screen, sends `POST /pair/enroll`, stores the issued token in the OS keychain, prompts for a label (default = client hostname), updates UI to "Connected · Clinic Server (LAN)."

QR path: deep-link handler `ferriscribe://pair?host=…&lan=…&ts=…&ports=…&code=…` registered at install. Scan with the system camera, tap the link, FerriScribe opens with the pairing prompt pre-filled. Manual paste-the-URL works too.

**Post-pair** the existing model pickers populate from the live server's `/api/tags` (Ollama) and `/v1/models` (LM Studio) endpoints; whisper uses the existing `whisper-1` constant. Old manual hostname/port fields remain visible-but-disabled with a "Replaced by paired connection. [Unpair]" hint.

## Heavy-box-as-client behavior

When sharing is enabled on a machine, the FerriScribe app on that same machine:

- Shows a small "Office server: this machine" badge in Settings → Sharing.
- Routes all STT and Ollama traffic to `127.0.0.1` directly, bypassing the auth proxy.
- Does not appear in its own Connected Clients list.
- LM Studio calls behave identically to today (the local app already talks to localhost).

If sharing is later turned off, the local FerriScribe falls back to whatever provider configuration was active before sharing was enabled.

## Threat-model honesty

The auth proxy + per-client tokens defend against:

- A casual eavesdropper on the clinic LAN who can reach the heavy box but does not have a paired token.
- A stolen / lost laptop: the server admin revokes its token from the Connected Clients panel; the laptop loses access immediately on next request.
- The current `OLLAMA_HOST=0.0.0.0` exposure on a clinic LAN with guest Wi-Fi or BYOD.

It does **not** defend against:

- A motivated attacker with full network access who captures plaintext HTTP between client and server. Calls flow over plain HTTP — TLS for paired connections is a sensible follow-up but not in this release. Tailscale already wraps its addresses in WireGuard, so Tailscale-only deployments get encryption transitively.
- A photographed enrollment code used within its 10-minute TTL before the legitimate clinician consumes it. Mitigated by one-shot semantics (second use rejected) but not eliminated.
- Malware on a paired client extracting the token from the OS keychain. Same threat model as the rest of FerriScribe.
- LM Studio traffic. LM Studio remote = trust the network.

This is the right shape for a clinic LAN / Tailscale deployment: a meaningful improvement over today's "open Ollama port, hope," without overclaiming.

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| Prebuilt whisper.cpp binary missing for a platform | Pinned manifest in repo; if a platform isn't listed, surface "you'll need to build whisper.cpp yourself" with a link. Most clinics get the happy path; unusual setups stay unblocked. |
| mDNS blocked by client-isolation Wi-Fi (UniFi, Meraki) | QR / code pairing is the universal fallback. Settings → Sharing client screen surfaces "No server found? Use a QR or code." |
| Windows firewall elevation refused | Try silent `netsh` first; clear copy-paste fallback with explanation if denied. |
| Photographed enrollment code | One-shot codes; 10-min TTL; long-lived tokens never displayed in any UI after first issuance. |
| Service file collision with a user-managed Ollama service | Wizard detects existing persistent service and skips its own; user keeps their setup. |
| Token store corruption | Server falls back to "no clients paired" state; clients see "unreachable" and the office-server admin re-issues codes. No silent unauthorized access. |
| whisper-cpp child crashes in a loop | Supervisor with capped exponential backoff; surfaces in the Sharing status panel; logs in `<app-data>/logs/` with no PHI (only counts/errors). |

## Testing strategy

- **Auth proxy** — unit tests for token validation (valid / expired / revoked / missing / malformed); integration test against a fake Ollama backend asserting forwarding semantics, header passthrough, and that Ollama itself never receives traffic with the bearer token.
- **Token store** — pairing state machine unit-tested: code issued → consumed → token issued → token validates → revoked token rejected.
- **Wizard step modules** — per-platform plist / unit / scheduled-task writers tested against fixture filesystems; actual `launchctl` / `systemctl` / `schtasks` calls behind a small trait, mocked in tests.
- **mDNS** — TXT-record encoding round-trip; "found server" deduplication; not testing the wire (rely on the crate).
- **Whisper supervisor** — clean-shutdown, crash-restart-with-backoff, log-rotation tests against a fake child that we control.
- **End-to-end smoke** — two FerriScribe instances on a dev machine, one in server mode; pair, transcribe a fixture audio file, get expected text back. No PHI fixtures.
- **Manual platform passes** — macOS arm64, Linux x86_64, Windows x86_64 before release. The persistent-service step in particular cannot be fully automated.

## Implementation surface (rough sizing)

Order matters but each is largely independent:

- New crate or module: `crates/sharing/` — auth proxy, mDNS advertiser/browser, pairing service, token store. (~1500–2000 LOC Rust.)
- whisper.cpp supervisor: extension to existing process-spawn patterns from diarization. (~300–400 LOC.)
- Service-installer module: per-platform plist / systemd unit / scheduled-task writers. (~400–600 LOC + small platform feature gates.)
- Frontend: Settings → Sharing pane (server side) and Settings → Sharing pane (client side). Reuses existing components heavily. (~600–900 LOC Svelte.)
- Wiring into existing Ollama and STT-remote providers: bearer header on paired connections, connection resolver. (~150–250 LOC.)
- Tests: ~30–40% of total LOC, biased toward auth-proxy and pairing state machine.

This is roughly comparable to the SQLCipher release in scope; reasonable for a single multi-task plan with subagent execution.

## Out of scope (explicit)

- TLS for paired connections. Sensible follow-up; today plain HTTP for paired traffic.
- LM Studio auth proxy.
- Multi-server / load balancing / failover between heavy boxes.
- Mobile clients.
- Tailscale install / management.
- Auto-updates of the bundled whisper.cpp binary. The wizard pins a known-good version per FerriScribe release; a re-run of the wizard updates it.
- Centralized clinic-wide settings (e.g., "all clinicians in this clinic share these prompt overrides"). One server, but each client is still an independent FerriScribe install.

## Open questions

None. Ready for implementation plan.
