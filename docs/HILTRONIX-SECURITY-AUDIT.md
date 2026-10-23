# Hiltronix OneKeePass security audit (Windows desktop)

Audit date: 2026-10-23. Source revision: `2026-10-23-4` (desktop v0.27.0).
Scope: Hiltronix Windows x64 portable desktop edition plus the pinned
Hiltronix `onekeepass-core` source at `1e57070fe7ca0c7d3ae4fb6fe38cc9ebbca95da5`.
Android is **not** covered by this Windows-only audit.

## Security model and limits

The edition is designed for **local KDBX operations, no built-in Internet
downloads, and deliberate URL opening through the user's external browser**.
There is no claim that the process has been denied network access by the
operating system. The executable still links libraries capable of networking.
The audit is a source/configuration review, **not** a runtime packet capture,
penetration test, dependency CVE scan, or formal proof of non-connectivity.

## Findings and disposition

| ID | Severity | Finding / evidence | Disposition |
|---|---|---|---|
| W-01 | High | `src-tauri/capabilities/migrated.json` granted `shell:allow-execute` to the main WebView despite no application need for shell command execution | **Fixed**: permission removed; `shell:allow-open` and `shell:default` retained for deliberate URL and file opening |
| W-02 | Medium | Browser native messaging service had several indirect startup call sites, and an existing browser manifest may survive from an older build | **Hardened**: Windows startup is denied in `browser_service/proxy_handler.rs`; command-level permission and installation entry points reject Windows calls; existing manifest cleanup is a separate manual step |
| W-03 | Medium | SSH Agent **Client Mode** previously ignored the per-key `Require Confirmation` flag when loading a key into an external agent | **Fixed**: such keys are rejected in Client Mode; Agent Mode preserves confirmation prompts and 60-second fail-closed timeout. SSH Agent Service itself remains available and disabled by default |
| W-04 | Medium | `onekeepass-core` feature `favicon` enables HTTP downloading as well as local image decoding. The desktop requests this feature to normalize a **local icon file** | **Unresolved / not reachable via registered Tauri command**: remove by separating local icon conversion from download capability in a subsequent core/lockfile migration; do not remove local icon import |
| W-05 | Medium | The desktop still depends on network-related packages, including SFTP/WebDAV in the shared core and `reqwest` required by the old update module | **No direct Windows path found** to network transports from registered commands. Remote-storage read/write/metadata and connect/list entry points are Windows-guarded; updater module is not registered or imported by `main.rs`. Dependency removal remains outstanding |
| W-06 | Medium | Dependency and toolchain versions are lockfile-based but not fully reproducible; no vulnerability scan or third-party advisory review has been executed | **Outstanding**: run RustSec `cargo audit`, JavaScript dependency audit, binary/SBOM inventory, and review advisories before release |
| W-07 | Medium | No Windows packet-level baseline has been established, including offline startup, unlocking, editing, URL opening, SSH Agent disabled/enabled, and browser manifest leftovers | **Outstanding**: perform firewall/WFP or packet-capture testing on a Windows test machine and check process tree/child processes |
| W-08 | Low | Internal native browser proxy is still compiled as a Tauri build sidecar prerequisite, despite being excluded from portable staging | **Constrained**: ZIP packing now asserts that `onekeepass-proxy.exe` is absent. Future clean-up can remove the unused sidecar build requirement and the Tauri `externalBin` reference together |
| W-09 | Low | Windows log files and temporary decrypted attachments are local resources and should be checked for secret leakage, retention, and ACLs | **Outstanding**: inspect file permissions and lifecycle with disposable data; never upload real passwords or logs containing secrets |
| W-10 | Informational | The SSH Agent speaks over Windows local OpenSSH named pipe and Pageant-compatible IPC; it is separate from SFTP/WebDAV and does not itself contact remote servers | **Retained per user**: disabled by default; local same-user clients may request signing; protect keys with `Require Confirmation` in Agent Mode |

## Source-level validation of network controls

- `src-tauri/src/main.rs` does not register an updater command or remote
  favicon downloader, and does not import `updater.rs`.
- The Windows URL-open functionality is intentionally delegated to the user's
  default browser, via `tauri-plugin-shell` `open`; **do not disable** it.
- WebView production CSP restricts `connect-src` to Tauri IPC endpoints, not
  external HTTP(S), in `src-tauri/tauri.conf.json`. This protects the WebView,
  but **does not** sandbox Rust/backend network-capable libraries.
- `src-tauri/src/remote_storage/mod.rs` rejects SFTP/WebDAV before read,
  write, create, metadata and merge operations. `commands.rs` independently
  rejects direct connect/list transport entry points on Windows.
- Browser integration on Windows is hidden in the UI, rejected by preferences,
  guarded at Tauri command entry points and prohibited at proxy start.
- GitHub Actions verifies the Windows portable package and SHA-256, but an
  Actions *build success* cannot establish runtime security or a passing
  unit-test suite.

## Acceptance work (must be performed separately)

1. With a disposable KDBX, test open/unlock/edit/save/reopen, templates,
   attachments, backups, TOTP, local icon imports and Windows geometry.
2. Check Help > About attribution/revision and user-requested entry URL
   opening, including after shell permission hardening.
3. Confirm SFTP/WebDAV and browser integration cannot be enabled using the
   UI or direct Tauri commands.
4. Ensure the portable ZIP has no browser proxy. Inspect any *older* browser
   native-messaging manifests installed by an upstream build separately.
5. Capture network connections from `OneKeePass.exe` while idle, opening,
   unlocking, saving and locking databases; separately test SSH agent modes
   and launching an entry URL. The *browser* may connect to the entry website
   intentionally, while `OneKeePass.exe` should not initiate Internet traffic.
6. Test permissions and lifetime of local logs, temporary attachments,
   clipboard data and the Windows Credential Manager session key.
7. Run dependency advisory scans for Rust, JavaScript, and packaged native
   runtime binaries. Review results and document any accepted exceptions.

## Change policy

Keep the original OneKeePass developers credited; retain Hiltronix attribution,
custom templates, KDBX compatibility, local icon import and SSH Agent Service.
Do not claim the resulting binary is fully offline/network-isolated until
the Windows runtime and dependency audit tasks above are completed.
