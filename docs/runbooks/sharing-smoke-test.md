# Sharing — Pre-merge Smoke Test

Run this before tagging a release that touches the sharing layer. Each
checkbox represents a discrete observation, not a single click. Total
time ~30 minutes on two machines.

## Setup

- One "office server" machine (any of: macOS arm64, Linux x86_64,
  Windows x86_64). Ollama installed and at least one model pulled.
- One "client" machine on the same LAN. (Two FerriScribe instances on
  the same machine is acceptable but does not exercise the cross-LAN
  path.)
- Optional: a Tailscale tailnet with both machines joined.

## On the office server

- [ ] Install FerriScribe (use the latest installer or `npm run tauri build`).
- [ ] Open Settings → Sharing → "This machine is the office server" →
      Start sharing.
- [ ] Observe: wizard surfaces no errors. Status panel shows green checks
      for Ollama, Whisper, mDNS, Pairing.
- [ ] Observe: macOS prompts to allow incoming network connections;
      Windows prompts for firewall elevation. Click Allow.
- [ ] Observe: pairing QR renders; 6-digit code visible below.
- [ ] Observe: "Connected clients" panel is empty.
- [ ] Inspect (manually): `<app-data>/rust-medical-assistant/bin/whisper-server`
      exists and is executable.
- [ ] Inspect (manually):
      - macOS: `~/Library/LaunchAgents/com.ferriscribe.ollama.plist` exists
        and `launchctl list | grep ferriscribe.ollama` shows the service.
      - Linux: `~/.config/systemd/user/ferriscribe-ollama.service` exists
        and `systemctl --user status ferriscribe-ollama` is active.
      - Windows: `schtasks /Query /TN "FerriScribe Ollama"` shows the task.

## On the client

- [ ] Install FerriScribe (matching version).
- [ ] Open Settings → Sharing → "This machine connects to an office
      server".
- [ ] Observe: the office server appears in the "Found on your network"
      list within 3 seconds.
- [ ] Click Connect; enter the 6-digit code; observe success message.
- [ ] Open Settings → Models. Confirm the model picker lists the
      models installed on the office server.
- [ ] Record a 5-second test audio clip. Confirm transcription returns
      reasonable text. (No PHI; use a known phrase like "the quick
      brown fox.")

## On the office server (post-pair)

- [ ] Connected clients panel now shows the laptop with its label.
- [ ] Click Revoke. Observe: row disappears.
- [ ] On the client, attempt another transcription. Confirm it now
      fails with a useful error (401 surfaced as "office server unreachable"
      or similar — not a silent hang).

## Tailscale path (if available)

- [ ] Disconnect both machines from the LAN; keep them on Tailscale.
- [ ] On the client, observe StatusBadge transitions from "Connected (LAN)"
      to "Connected (Tailscale)" within ~5 seconds.
- [ ] Re-record. Confirm transcription works over Tailscale.

## QR / deep-link path

- [ ] On the office server, copy the `ferriscribe://pair?...` URL from
      the QR display.
- [ ] On a third client (or after unpairing), open the URL via the
      OS's URL handler. Confirm FerriScribe opens with the pair screen
      pre-filled.
- [ ] Observe: pair completes without typing the code separately.

## Stop sharing

- [ ] On the office server, click "Stop sharing".
- [ ] Observe: status panel disappears; whisper-server child process
      exits (`ps aux | grep whisper-server` returns nothing); mDNS
      advertisement stops (other clients no longer find this server).
- [ ] Restart the office server machine. Observe: Ollama starts on
      reboot via the persistent service. (FerriScribe sharing does NOT
      auto-restart — that's by design; the user has to open the app.)

## Failure-mode probes

- [ ] On the office server, kill the whisper-server child manually:
      `pkill whisper-server`. Observe: supervisor restarts it within
      ~1–2 seconds; new transcription requests succeed after the
      restart.
- [ ] Stop sharing while a transcription is in flight. Observe: client
      sees a clean error, not a hang.
- [ ] On a fresh paired client, delete the keychain entry
      (macOS: `security delete-generic-password -a sharing-bearer`).
      Observe: subsequent calls fail; user is prompted to re-pair.

If any checkbox fails, file an issue tagged `sharing` and do not
tag the release.
