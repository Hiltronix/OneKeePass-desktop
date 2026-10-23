# Hiltronix OneKeePass security audit (Windows desktop)

Audit date: 2026-10-23. Source revision: `2026-10-23-5` (desktop v0.27.0).
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
| W-08 | Low | Windows portable builds previously compiled the disabled browser proxy unnecessarily | **Remediated in source**: Windows-specific Tauri config clears `bundle.externalBin` and its packaging script no longer compiles the proxy. Other platforms retain their original sidecar; verify with the next Windows build |
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

## Dependency scan results: 2026-10-23

GitHub Actions dependency scan [#39962965134](https://github.com/Hiltronix/OneKeePass-desktop/actions/runs/39962965134) completed and archived its reports. **The green workflow result does not mean the dependency inventory passed.** The scan deliberately continues after vulnerability findings.

| Inventory | Scanner exit | Findings |
|---|---:|---|
| Desktop Rust Cargo.lock | 1 | 19 advisory/package records (including duplicate advisory IDs for multiple versions), plus 5 unmaintained, 2 unsound and 2 yanked warnings |
| Browser-proxy Rust Cargo.lock | 1 | `RUSTSEC-2026-0007` in `bytes 1.10.1`; proxy is not built or shipped by the Windows-only portable pipeline after W-08 remediation, although its separate lockfile still exists for other platforms |
| JavaScript Yarn dependencies | 14 | Multiple high/moderate advisories, including `serialize-javascript`, `fast-uri`, `ws`, `yaml` and `ajv` |

Important desktop Rust findings include `RUSTSEC-2023-0071` (`rsa` timing attack), `RUSTSEC-2026-0385` (`ctutils` nonconstant-time operations), `RUSTSEC-2026-0194/0195` (`quick-xml` denial of service), advisories affecting `quinn-proto`, and `rustls` / WebPKI trust and handshake issues. **Presence in a lockfile does not establish that a vulnerable execution path is reachable in this Windows build.** Determine which packages are included in the actual executable, their enabled features, whether affected APIs are called, and the patched versions before selecting upgrades.

The Yarn scan covers development/build dependencies as well as application packages; identify production exposure separately. Do not reflexively update all locked versions in a password manager without KDBX compatibility and signing regression tests.

**Next remediation gate:** prioritize cryptography, certificate validation, parsers consuming untrusted files, and signing/SSH-agent dependencies; upgrade or remove vulnerable dependencies in controlled groups; rebuild, retest and rescan. The scan findings remain **open** as of this audit.

### Additional code review notes

- **SSH Agent retained:** Agent Mode's Windows named pipe / Pageant transports are local IPC, but same-user applications can ask for signatures from available keys. Do not equate local IPC with zero risk. Client Mode exports keys into an external agent; confirmation-only keys are now rejected in that mode.
- **Tauri permissions:** `shell:allow-open` is intentionally retained for entry URL opening. `shell:default` and `process:default` should be minimized after validating which concrete operations the installed plugins expose. CSP restrictions cover the WebView, not privileged Rust code.
- **Sensitive local data:** user-verified KDBX read/write and desktop-mobile synchronization are functional. This does not verify the permissions or secure deletion of temporary attachment files, log files, copied passwords and backups.
- **Boundary:** static review and advisory scanning do not prove no outbound network traffic. Windows WFP/firewall process tracing with SSH Agent on/off and external browser URL opening remains a release-blocking verification step for any claim of complete isolation.

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

## Controlled Rust remediation in progress

The Windows-only portable build now excludes compiling the disabled browser proxy and uses `tauri.windows-offline.conf.json` to clear the Tauri `externalBin` sidecar. The direct desktop `reqwest 0.12` dependency (left over from the unlinked updater) was removed from the manifest. A one-shot GitHub Actions resolver (`hiltronix-lockfile-remediation.yml`) regenerates `src-tauri/Cargo.lock` using Cargo and attempts compatible patched versions of `ctutils`, `rustls`, `rustls-webpki`, `quinn-proto`, `webpki-roots`, and `webpki-root-certs`. It reports and skips any package whose constraints reject the requested version; a green resolver run is **not** a substitute for a succeeding Windows build and a new advisory scan.

No forced RSA major-version change is attempted: RustSec RUSTSEC-2023-0071 lists no patched version, and altering SSH signing could break authentication. Older `quick-xml` release lines remain an explicit separate issue pending analysis of pinned upstream/core dependency constraints. Runtime network isolation remains unverified.

### JavaScript toolchain dependency reduction

The root `webpack` and `webpack-cli` development dependencies were removed; the actual Windows production UI is generated by `shadow-cljs release app` with a `:browser` target, not webpack. A narrowly scoped GitHub Actions workflow regenerates `yarn.lock` and validates it using Yarn Classic. This should remove the unused webpack-only dependency graph, including several previously flagged development packages. **Verify the actual remaining Yarn advisories after the new scan; do not assume they all disappear.** The separate `src-js` toolchain and non-Windows build scripts are not modified.
