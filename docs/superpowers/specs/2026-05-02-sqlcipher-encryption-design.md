# SQLCipher Database Encryption at Rest — Design

**Date:** 2026-05-02
**Status:** Approved (ready for implementation plan)
**Sprint:** Sprint 2 architectural deferral (resolved)

## Problem

Today the SQLite database at `<app_data_dir>/medical.db` is stored in plaintext on disk. The application is HIPAA-adjacent: it holds patient transcripts, SOAP notes, structured medications/allergies/conditions, and recording metadata that often includes patient names. Disk-level encryption (FileVault / BitLocker / LUKS) protects a powered-off device, but does not defend against:

- Forensic recovery from an off device whose disk encryption is weak or missing.
- Backups copied to non-encrypted external media.
- Accidental cloud sync of the data directory.
- Multi-user systems where another user's account or weak filesystem permissions expose the file.

This release adds transparent at-rest encryption of the SQLite database via SQLCipher, with the encryption key stored in the operating system keychain (macOS Keychain Services, Windows Credential Manager, Linux Secret Service via `keyring`). Existing plaintext databases are migrated forcibly on first launch after upgrade.

## Threat-model honesty

The OS-keychain-backed approach defends against:
- Forensic recovery from a powered-off device.
- Backups exfiltrated without the keychain.
- Multi-user filesystem snooping.
- Cloud-sync exfiltration of the DB file alone.

It does not defend against:
- Malware running as the user that can prompt the OS keychain (the keychain may grant access without explicit interaction once initially trusted).
- A live unlocked device in the attacker's hands.

This is the correct shape for a clinician workstation app: a meaningful additional layer of defense beyond disk encryption, without claiming protections it cannot deliver.

## Goals

- Transparent encryption of `medical.db` and all sidecar files (`-wal`, `-shm`).
- Forced migration on first launch after upgrade — no opt-in toggle.
- Encryption key generated randomly per install, persisted in the OS keychain via the `keyring` crate.
- No user passphrase. No cloud key escrow.
- Recovery dialog when the encrypted DB exists but the keychain entry is missing.
- Behavior preserved — every existing app feature works identically through the encrypted layer.

## Non-goals

- Migrating the existing `KeyStorage` (API-key encryption in `crates/security/src/key_storage.rs`) to OS keychain. It continues using its current machine-ID-PBKDF2 scheme. Sensible follow-up release.
- Recovery passphrase as a second decryption slot.
- Auto-export of plaintext backups (would defeat the encryption story).
- Periodic re-keying / automatic rotation. A manual "Re-encrypt with new key" Settings button is included; no schedule.
- Cross-device keychain sync.
- Hardware-backed keys (Secure Enclave, TPM).

## Decisions

| # | Decision |
|---|---|
| Q0 | Approach: OS keychain via the `keyring` crate (path B from the meta-question). |
| Q1 | Key generation: 32 random bytes generated on first launch, stored in the OS keychain. No user passphrase. |
| Q2 | Migration: forced on first launch after upgrade — backup → encrypt → verify → swap → delete backup. |
| Q3 | Recovery: cryptographic shredding accepted. On missing keychain entry, surface a `[Restore from backup] [Wipe and start fresh] [Quit]` dialog. |
| Q4 | API-key `KeyStorage` stays on its current scheme; out of scope. |

## Architecture

```
At-rest encryption layer:
  rusqlite (bundled-sqlcipher feature)  ← drop-in replacement for SQLite
  PRAGMA key applied on every pooled connection

Key management:
  crates/security/src/keychain.rs (new)  ← thin wrapper over the `keyring` crate
    ├─ get_or_create_db_key()  → 32 random bytes, persisted in OS keychain
    └─ wipe_db_key()           → for the "Wipe and start fresh" recovery path

DB initialization (crates/db/src/pool.rs):
  create_pool(db_path, db_key)
    ├─ r2d2_sqlite manager with .with_init() that applies PRAGMA key + existing pragmas
    └─ first-run migration: see "Migration flow" below

Existing API-key KeyStorage (crates/security/src/key_storage.rs):
  Unchanged in this release.
```

### Boot flow on every launch

1. State init resolves the app data dir.
2. Asks `keychain::get_or_create_db_key()` for the key.
3. Detects DB state at the path:
   - **Plaintext DB exists, no keychain entry yet** → first-time encryption migration.
   - **Encrypted DB exists, keychain entry present** → normal start; pool applies `PRAGMA key`.
   - **Encrypted DB exists, keychain entry missing** → recovery dialog.
   - **Neither exists** → fresh install; create encrypted DB.

## Migration flow (forced, on first launch after upgrade)

```
1. Confirm DB is plaintext        sqlite3_open + try a no-key SELECT; if it succeeds, plaintext.
2. Acquire a process-level lock   Prevent two instances racing the migration.
3. Generate the new key            32 random bytes via rand::thread_rng + ChaCha20 fallback.
4. Store the key in the keychain   keyring::Entry::new("rustMedicalAssistant", "db-key").set_secret(&bytes)
   ── If the user denies access on first prompt: abort migration, log error,
      continue running with plaintext DB. Surface a non-blocking toast:
      "Database encryption could not be enabled — denied keychain access. Retry in Settings."
5. Backup plaintext DB             Copy <data_dir>/medical.db → <data_dir>/medical.db.pre-encryption.bak
                                    (Lives only briefly; deleted in step 9.)
6. Create empty encrypted DB       <data_dir>/medical.db.encrypting
                                    sqlite3_open + PRAGMA key='<hex>' + apply migrations
7. sqlcipher_export()              ATTACH plaintext as 'plaintext'; SELECT sqlcipher_export('main');
                                    Copies all tables, indices, triggers, views, FTS5 virtual tables.
8. Verify                          Re-open <medical.db.encrypting> with the keychain key.
                                    Run table-level row-count check: every table in plaintext
                                    has the same count in encrypted. If any mismatch: abort.
9. Atomic swap                     fsync + rename <medical.db.encrypting> → <medical.db>
                                    Delete <medical.db.pre-encryption.bak>.
10. Emit progress events           "encryption-progress" with status: "started" / "completed"
                                    Frontend renders a brief progress overlay in App.svelte.
```

**Failure handling (any step after backup):**
- Restore: rename `<medical.db.pre-encryption.bak>` back to `<medical.db>`, delete `<medical.db.encrypting>`, leave the keychain entry alone (next launch retries).
- Surface error toast naming the failure stage.
- App continues running on the plaintext DB; do not crash.

**WAL/journal files:** SQLCipher encrypts `.db-wal` and `.db-shm` automatically.

**Concurrency invariant:** the `r2d2` pool is built only after migration completes. Migration runs synchronously in `state::init()` before any connection is handed out.

## Recovery flow (encrypted DB exists, keychain entry missing)

Plausible causes: user manually wiped the keychain entry, OS reinstall without keychain restore, Time Machine restore that copied the data dir but not the keychain, app reinstalled on a new user account.

### Frontend recovery dialog

```
┌─────────────────────────────────────────────────────────────┐
│ Encrypted database, missing access key                     │
│                                                             │
│ The database at <path> is encrypted, but the access key    │
│ stored in your system keychain is missing or inaccessible. │
│ The data cannot be decrypted without it.                    │
│                                                             │
│ This usually means the keychain was reset, the app was      │
│ reinstalled, or the data folder was copied from another     │
│ machine.                                                    │
│                                                             │
│ [ Restore from backup file ]  [ Wipe and start fresh ]     │
│ [ Quit ]                                                    │
└─────────────────────────────────────────────────────────────┘
```

**Action behaviors:**

- **Restore from backup file** — opens a native file picker. User picks a `.db` or `.db.bak` file. App attempts to open it as plaintext; if successful, copies into place and runs migration. If the picked file is itself encrypted (e.g., from a different install with a different key), warn and reject — only plaintext-restorable backups are supported.
- **Wipe and start fresh** — typed confirmation (`"Type DELETE to confirm"`), then deletes `medical.db` + sidecars, generates a new keychain entry + new encrypted DB, restarts.
- **Quit** — closes the app for manual investigation.

### Implementation surface

- New Tauri command `recover_database` in `src-tauri/src/commands/recovery.rs`. Variants: `restore_from_path(path: PathBuf)`, `wipe_and_reset()`. Gated on the recovery state.
- New Svelte component `src/lib/dialogs/DatabaseRecoveryDialog.svelte`. Renders only when `App.svelte` receives a `database-recovery-needed` event from the backend on boot.
- `state::init()` returns `Ok(AppState)` for normal boots or `Err(InitError::DatabaseRecoveryNeeded { reason })` to signal the recovery state. Caller (in `lib.rs`) emits the event without starting the rest of the app.

### Settings UI surface

A new "Database security" section in `settings/General.svelte`:

```
Database encryption: ✓ Encrypted (AES-256, key in OS keychain)
[ Re-encrypt with new key ]   ← rotates the key; rare-use button
[ View backup recommendations ]   ← tooltip / link to docs
```

Status indicator only; the recovery dialog handles the actual loss case from launch.

## Build & dependencies

**`crates/db/Cargo.toml`** — feature swap:
```toml
rusqlite = { version = "0.32", features = ["bundled-sqlcipher", "vtab", "functions"] }
```

**`crates/rag/Cargo.toml`** — must match (currently `["bundled"]`):
```toml
rusqlite = { version = "0.32", features = ["bundled-sqlcipher"] }
```

If `bundled` and `bundled-sqlcipher` features collide via Cargo's feature unification, the workspace will fail to build until both crates align. Verify after the swap that no other crate in the workspace transitively pulls in `rusqlite` with `bundled` only.

**`crates/security/Cargo.toml`** — add:
```toml
keyring = "3"
```

**Binary size impact:** ~5–10 MB increase (SQLCipher's bundled OpenSSL). Document in the version-bump commit message.

## Error handling

| Scenario | Behavior |
|---|---|
| Keychain access denied at first-launch migration | Migration aborts cleanly. Plaintext DB stays. Non-blocking toast: "Database encryption could not be enabled — denied keychain access. Retry in Settings." App functional. |
| Migration fails after backup is taken | Restore backup, surface toast with stage of failure, continue running on plaintext. |
| Encrypted DB corruption (SQLCipher returns "file is not a database") | Treat as missing key — show the recovery dialog. |
| `keyring` crate panics on a malformed entry | Catch the panic via `std::panic::catch_unwind` at the boot path, fall through to recovery dialog. |
| Keychain entry exists but DB file is plaintext (e.g. user replaced the DB file) | Show a confirmation dialog: "Database appears unencrypted but a key exists. Re-encrypt? [Yes / Wipe / Quit]." |

## Testing

### Rust unit tests

- `crates/security/src/keychain.rs`: `get_or_create_db_key` round-trips a key; `wipe_db_key` clears it. Use `keyring`'s `MockKeyring` backend in test setup.
- `crates/db/src/encryption.rs`: `apply_pragma_key` correctly encodes the key as a hex string for `PRAGMA key`.

### Rust integration tests (`crates/db/tests/encryption.rs` — new)

- Migration happy path with seeded data: plaintext → encrypted → verified → backup deleted.
- Verification mismatch triggers restore.
- Keychain denial leaves plaintext intact.
- Encrypted DB opens correctly across `r2d2` pool reconnections.

### Frontend vitest

None required (no new pure-helper code). `DatabaseRecoveryDialog.svelte` is UI-only; manual smoke tests cover it.

### Manual smoke tests after merge

1. Fresh install → DB created encrypted from the start.
2. Upgrade with existing plaintext DB → see encryption progress overlay → DB encrypted, normal use works.
3. Delete keychain entry manually (`security delete-generic-password -s rustMedicalAssistant -a db-key` on macOS) → relaunch → recovery dialog appears → "Wipe and start fresh" works.
4. Settings → "Database encryption" panel shows "Encrypted".

## Rollout

- One feature branch, multiple commits (exact count in the implementation plan).
- Version bump to `0.10.13`.
- No DB schema migration; only the storage format changes.
- No frontend API changes for existing recordings — they keep working through the encrypted layer transparently.

## Files touched (anticipated)

**Created:**
- `crates/security/src/keychain.rs` — `keyring` wrapper.
- `crates/db/src/encryption.rs` — encryption module: PRAGMA key application, migration helpers.
- `crates/db/tests/encryption.rs` — integration tests.
- `src-tauri/src/commands/recovery.rs` — recovery Tauri commands.
- `src/lib/dialogs/DatabaseRecoveryDialog.svelte` — recovery UI.

**Modified:**
- `crates/db/Cargo.toml` — `bundled` → `bundled-sqlcipher`.
- `crates/rag/Cargo.toml` — same feature swap.
- `crates/security/Cargo.toml` — add `keyring = "3"`.
- `crates/db/src/pool.rs` — `create_pool` accepts a key; `apply_pragmas` adds `PRAGMA key`.
- `crates/db/src/lib.rs` — export the encryption module.
- `crates/security/src/lib.rs` — export `keychain`.
- `src-tauri/src/state.rs` — orchestrate keychain lookup, migration call, recovery signaling.
- `src-tauri/src/lib.rs` — handle the `DatabaseRecoveryNeeded` boot result; emit event.
- `src-tauri/src/commands/mod.rs` — register `recovery` commands.
- `src/lib/components/settings/General.svelte` — new "Database security" panel with status + rekey/backup buttons.
- `src/App.svelte` — listen for `database-recovery-needed` event; render the recovery dialog.
- `src-tauri/Cargo.toml`, `package.json`, `src-tauri/tauri.conf.json`, `Cargo.lock` — version bump to `0.10.13`.
