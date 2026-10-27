# Hiltronix OneKeePass security audit (Windows desktop)

Audit date: 2026-10-23. Source revision: `2026-10-26-01` (desktop v0.27.0).
Scope: Hiltronix Windows x64 portable desktop edition plus the pinned
Hiltronix `onekeepass-core` source at `a4f0f2b74d893f3a4b8b5052c960e2497177fd2f` (quick-xml 0.41 branch-tested commit, now reachable from core main).
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
| W-05 | Medium | Network-capable libraries remain transitively available through SFTP/WebDAV in the shared core and Tauri | **Constrained**: Windows remote-storage entry points are guarded, the updater is not linked, and the unused direct desktop reqwest dependency is removed. Further unused-core feature removal remains outstanding |
| W-06 | Medium | Dependency and toolchain versions are lockfile-based but not fully reproducible | **Partially remediated**: RustSec and Yarn scans executed; many compatible patches and unused-dependency removals landed. Outstanding advisories, locked-toolchain verification, binary/SBOM inventory and dependency provenance review remain |
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

### Remediation batch results (2026-10-23)

Source commits removed the disabled browser proxy from the Windows build pipeline and obsolete desktop `reqwest 0.12`; Cargo regenerated the locked Rust dependency closure. Security patch versions are `ctutils 0.4.4`, `quinn-proto 0.11.18`, `rustls 0.23.46`, `rustls-webpki 0.103.16`, `webpki-roots 1.0.10`, and `webpki-root-certs 1.0.10`.

The production frontend uses `shadow-cljs` rather than webpack. The unused `webpack` and `webpack-cli` build dependencies and their JavaScript transitive closure have been removed. Remaining `ws` and `yaml` dependencies are locked to patched compatible releases `8.21.3` and `1.10.3`, respectively.

**Remaining risk and follow-up:** The shared-core KDBX parser is now upgraded to `quick-xml 0.41.0`, closing its duplicate-attribute denial-of-service advisory. The lockfile still contains `quick-xml 0.39.4` through Linux Wayland tooling; this is not the Windows KDBX parser. The RSA timing advisory `RUSTSEC-2023-0071` has **no patched crate version** and still needs exploitability/mitigation review. Do not swap cryptographic implementations blindly.

The earlier advisory scan totals describe the **pre-remediation** dependency inventory. Fresh RustSec and Yarn scans and a Windows build on the final lockfiles are mandatory before accepting the updated build. SSH Agent Service and user-triggered URL opening remain functional features that must be preserved.

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

### Targeted JavaScript transitive patch resolutions

After eliminating webpack and its transitive closure, only `ws 8.20.0` and `yaml 1.10.2` remained among the previously flagged Yarn packages. The root manifest now selects `ws 8.21.3` (fixing the fragmented-message resource exhaustion advisory) and `yaml 1.10.3` (fixing the deep-nesting stack overflow advisory) using Yarn Classic `resolutions`. Both preserve their dependency major versions. Verify the regenerated lockfile and a fresh advisory scan before calling the JavaScript tree clean. `resolutions` should be revisited when the upstream ranges themselves are updated.

## Validated quick-xml 0.41.0 remediation (2026-10-23)

- **Root cause:** the older plain `quick_xml::Reader` iterated untrusted KDBX XML attributes with a quadratic duplicate-name check (RUSTSEC-2026-0194). The separate namespace resolver allocation issue (RUSTSEC-2026-0195) does not affect the core's plain `Reader`; the core does not use `NsReader`.
- **Resolution:** upgrade the pinned Hiltronix shared core's `quick-xml` dependency from `0.37.5` to `0.41.0`; adapt the text decoding and `GeneralRef` event APIs without altering KDBX encryption, stored templates, SSH Agent Service or URL opening.
- **Core CI:** [run #39992949355](https://github.com/Hiltronix/OneKeePass-onekeepass-core/actions/runs/39992949355), **294 passed, 0 failed, 70 ignored**, including explicit tests for XML escapes and duplicate-attribute handling. One unrelated legacy time-zone-dependent test was made portable.
- **Desktop CI:** [Windows portable run #39993374728](https://github.com/Hiltronix/OneKeePass-desktop/actions/runs/39993374728) passed compilation, archive hash checking and artifact upload for the patched core.
- **Final dependency scan:** [run #39993374809](https://github.com/Hiltronix/OneKeePass-desktop/actions/runs/39993374809) found **4 remaining Rust advisory/package records**: 2 for `quick-xml 0.39.4` from Linux Wayland tooling, and 2 RSA timing advisory records. Yarn reported 0 vulnerabilities. Other unmaintained, unsound and yanked package warnings remain open for triage.
- **Limit:** a successful compile and unit-test suite is not a complete KDBX interoperability/regression test against every user's database. Use disposable databases for the first run; user-controlled synchronized KDBX data should remain backed up.


## WebView2 per-application network restrictions (2026-10-23; revision 2026-10-23-11)

Windows WebView2 has **app-specific startup browser arguments** in `src-tauri/tauri.conf.json` under the single Tauri window's `additionalBrowserArgs`. These apply when **this application** creates its WebView2 browser process, not to Microsoft Edge, PowerToys, or every WebView2 process on the computer. The property is Windows-only; Linux/macOS browser implementations do not use these flags.

- Preserve the WebView2/Wry defaults: `--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection`.
- Suppress optional Chromium background communication: `--disable-background-networking --disable-component-update --disable-sync`.
- Route ordinary WebView2 HTTP(S) requests to a deliberately absent local proxy: `--proxy-server=http://127.0.0.1:9` (no fallback proxy configured). The loopback target is not an Internet destination. Chromium's implicit localhost bypass preserves Tauri's local IPC; keep the existing `useHttpsScheme` and production CSP unchanged.

This is **defense in depth, not a formal network sandbox**: browser switches can change in future WebView2 versions, WebView2 internals may bypass proxy settings for some services, and native Rust dependencies are not subject to the proxy. Deliberately opening an entry URL in the user's external default browser remains allowed. SSH Agent local IPC and local KDBX operations remain unchanged. Windows WebView2 Runtime updates remain managed by Microsoft outside this application.

Validation required: verify OneKeePass can launch, unlock/edit/save a disposable KDBX, import local icons, use SSH Agent Service if applicable, and open entry URLs in the external browser. On the test PC inspect the WebView2 process command line for the arguments and capture external WFP traffic with exact OneKeePass/WebView2 PID ancestry. **Do not claim confirmed zero outbound connections until runtime tests pass**.

Refs: https://v2.tauri.app/reference/config/#windowconfig ; https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/winrt/microsoft_web_webview2_core/corewebview2environmentoptions


## Welcome-screen revision display (2026-10-23; revision 2026-10-23-12)

The initial Welcome/start screen displays a centered `Revision: <value>` directly beneath the semantic application version `0.27.0`. Both this label and Help > About use the **same** value from `src-cljs/main/onekeepass/frontend/build_revision.cljs`, preventing diverging hard-coded UI revision strings. Functional acceptance: verify the Welcome/start screen and Help > About both show `2026-10-23-12` after the Windows portable build; WebView2 restrictions from revision 11 are unchanged.


## Windows sensitive-data and SSH Agent source review (2026-10-23; revision 2026-10-23-13)

**Scope:** Windows x64 portable source review; runtime behavior still requires a disposable-database test. No live secrets or production KDBX files were inspected.

- **High / remediated in the desktop entry point:** `save_attachment_as_temp_file` previously forwarded a KDBX-controlled attachment name to the generic shared-core API, which used `%TEMP%/okp_cache/<name>` with `File::create`. Absolute paths, traversal components and collisions could overwrite unrelated files. The Windows desktop now creates a unique UUID-based filename with `create_new` and a restricted, validated extension, and invokes the existing `save_attachment_to_writter` function directly. Partial files are removed on write failure. Explicit attachment **Save As** intentionally continues to write to the path the user chooses.
- **Important remaining caveat:** opening attachments externally still writes decrypted data to disk; it remains accessible to other same-user applications and may be recoverable after deletion on SSDs, in antivirus caches, filesystem snapshots or from external editors. The core `okp_cache` sweep runs at startup and app shutdown, not continuously. The portable `onekeepass-data` directory and user-chosen backup locations have their normal Windows filesystem ACLs; review their privacy on shared machines/removable drives.
- **Clipboard:** sensitive entry-field copying calls `write_sensitive_to_clipboard`, which schedules conditional clearing (default 30 seconds) rather than blindly overwriting a newer clipboard value. OS clipboard history/sync may preserve captured content independently; disable those Windows features where confidentiality is paramount. Other generic copy operations may have different sensitivity; perform an end-to-end clipboard test.
- **Logging:** production uses Info-level size-rotated logs (5 MiB each, 2 retained rotations) in the app data directory. No evidence of KDBX plaintext passwords being deliberately logged in the reviewed Rust paths, but database/attachment metadata and filesystem paths can appear, and log-file ACLs have not been independently tested. SSH-agent warnings now avoid including user-defined key entry titles where possible.
- **Credential Manager:** the session encryption key protecting the composite key in memory uses a `CRED_PERSIST_SESSION` generic credential (cleared on logoff) with an in-memory `SecVec` fallback. Current implementation still creates temporary ordinary `Vec<u8>` buffers when hex-encoding/decoding the key; secure buffer lifecycle/zeroization remains for a separate validated change. Credential Manager is not the store for users' plaintext database passwords.
- **SSH Agent:** Agent Mode uses the local OpenSSH named pipe and Pageant IPC, not network sockets; it is disabled by default. Pageant checks mapping-owner SID against the current user. OpenSSH transport uses the underlying `ssh-agent-lib` default named-pipe security descriptor and needs a Windows ACL/impersonation verification test. Confirmation-required keys prompt with a 60-second fail-closed timeout; now each sign rechecks the current confirmation requirement and key-store generation, invalidating approvals when keys reload, lock, expire or agent stops. Invalid nonempty Agent Lifetime strings reject the key instead of providing unlimited signing. Client Mode refuses keys requiring confirmation; an external agent's security posture remains outside OneKeePass's control.

**Validation gate:** Windows CI and targeted Rust unit tests where available, test startup and saving/opening harmless attachments whose names contain traversal syntax, test normal extension mapping, lock/reload a key during a pending SSH sign confirmation and confirm failure, test valid and invalid lifetimes, inspect named-pipe owner/DACL, verify stale temp cleanup and clipboard timer, and ensure Help > About and Welcome show revision `2026-10-23-13`. The RSA timing advisory and dependency warnings remain unresolved. Do not claim all temporary secret copies are cleared or that Windows runtime testing has passed just from CI.


## Windows credential-buffer and Pageant bounds hardening (2026-10-23; revision 2026-10-23-14)

- **Windows session-key buffers:** incoming `store_key` bytes and intermediate `cred_write` / `get_key` / `copy_key` hex buffers are now wrapped in `secstr::SecVec` to zero their owned memory when released, including most error paths. The returned `get_key` plaintext `Vec<u8>`, Windows-owned `CredReadW` allocation, and in-process copies elsewhere remain outside this guarantee; these changes do not assert comprehensive RAM erasure.
- **Pageant malformed-input hardening:** `WM_COPYDATA` mapping names now require an in-bounds terminal NUL, no embedded NUL, and a maximum 256-byte length before the Win32 NUL-terminated API is called. `MapViewOfFile` now requests the full 8192-byte Pageant mapping, rejecting undersized mappings rather than dereferencing a truncated region. The shared handler retains same-user mapping-owner SID validation, bounded frame handling, explicit confirmation, and timeouts.
- **Outstanding:** default security ACL for OpenSSH named pipe is controlled by `ssh-agent-lib`. Microsoft documents default named-pipe ACLs that include some access for Everyone and Anonymous, so explicit DACL inspection or a custom same-user security descriptor is required before calling the transport strictly same-user. Test the raw Windows pipe with a disposable key using a separate Windows user account before tightening ACLs. Remote pipe clients should also be excluded. User-defined passphrases / protected fields are still present transiently as ordinary buffers in parts of upstream libraries.
- **Verification:** build Windows portable ZIP from this source, inspect MSVC compile errors and package hash, test local KDBX open/edit/save, credential-manager re-unlock, SSH OpenSSH named-pipe and Pageant signing/confirmation, invalid lifetime rejection, and bounded Pageant frames with disposable keys. Revision on Welcome and About must both be `2026-10-23-14`.



## Windows OpenSSH agent startup deadlock (2026-10-26; revision 2026-10-26-01)

Windows production logs on October 23 and October 26 captured a consistent sequence:

```text
[ERROR] SSH agent failed to start: timed out waiting for
        \\.\pipe\openssh-ssh-agent bind result: timed out waiting on channel
[INFO] SSH agent listening on \\.\pipe\openssh-ssh-agent
```

The old code spawned Tokio named-pipe binding on Tauri's shared async runtime, then used synchronous `recv_timeout(5s)` to await completion. When invoked on a Tauri runtime worker, that block could starve the binding task. The caller marked startup failed, cleared the key store and discarded its shutdown handle; the orphan task later acquired the pipe, so `ssh-add -l` could connect but report `The agent has no identities`.

**Remediation:** Windows OpenSSH binding now runs on its own named OS thread with a dedicated current-thread Tokio runtime. Startup confirmation is sent only after successful binding. If the startup caller times out and drops the receiver, the worker drops the late-bound listener without servicing requests. The app stores the worker handle, issues a non-losing `Notify::notify_one` on stop, and joins the thread before allowing an immediate restart. Failed starts now show their error, and an enabled-but-not-running agent shows a warning in the SSH Agent settings UI, instead of suppressing diagnostics.

**Build acceptance:** MSVC compile and portable ZIP packaging must pass. On Windows, launch revised OneKeePass, open the existing disposable SSH Key entry with `Add to SSH Agent` enabled, and verify the settings display the pipe and nonzero key count; then `ssh-add -l` should report the expected public-key fingerprint. Test `ssh-add -T <existing-public-key-file>` to verify signing and confirmation; lock database and ensure the key disappears, then unlock and verify it returns. Check stop/start without delays, no bind timeout in log, and confirm separate OpenSSH service remains stopped. The source change is not a claim that runtime validation has passed.

**Windows named-pipe permissions:** custom DACL enforcement remains a separate audit item; this fix does not change the security descriptor and does not prove isolation from other local accounts.


## Follow-up audits (2026-10-27)

Network isolation and Windows OpenSSH named-pipe security have received a new source-level review. See [HILTRONIX-WINDOWS-SECURITY-REVIEW-2026-10-27.md](HILTRONIX-WINDOWS-SECURITY-REVIEW-2026-10-27.md) for inspected control points, findings and acceptance criteria.

- **Network:** updater, browser native messaging and Windows remote-storage entry points are disabled and WebView2 restrictions are configured; no user-machine, process-attributed WFP/ETW network capture has been performed. **Runtime verification open.**
- **Named pipe:** OpenSSH pipe currently binds with `ssh-agent-lib` default security descriptor, without an explicit user-only DACL or verified per-client token restriction. Default permissions can include broader read access. **ACL hardening and effective-access verification open.** Pageant's mapping-owner check is independent.
- **Evidence helper:** `scripts/Audit-WindowsSecurity.ps1` queries the named-pipe DACL by handle and samples process-attributed TCP endpoints without altering Windows configuration. Polling does not capture UDP or all short-lived TCP connections; a second-user negative test is still necessary.

Do not close W-07 or the named-pipe security issue on the strength of source review or a green build alone.

## Named-pipe restricted-DACL remediation candidate (2026-10-27-03)

Current Windows source builds each `\\.\pipe\openssh-ssh-agent` instance with a protected same-account/SYSTEM/Administrators DACL, using Tokio's `create_with_security_attributes_raw` API; the SDDL is derived from the current process-token user SID and does not include Everyone or Anonymous. Existing `ssh-agent-lib` protocol handling remains unchanged. **Unverified until the Windows build and same-account/other-account runtime tests pass.** See the October 27 Windows security review for acceptance requirements.


## P-01 follow-up runtime verification (2026-10-27)

User-provided Windows output from revised `2026-10-27-03` confirms the live `\\.\pipe\openssh-ssh-agent` ACL is protected and grants access only to SYSTEM, Builtin Administrators and the current Windows account. There are no Everyone or Anonymous ACEs. `ssh-add -l` enumerates the user's two ED25519 identities, and KiTTY and WinSCP both authenticate using OKP keys after the change. The Windows build succeeded in CI.

**Status:** The restricted ACL and supported-client compatibility are validated on the user's machine. A *different, non-administrator account* has not yet tested access denial. The network audit observed zero sampled TCP connections, but DNS/UDP and transient traffic still require WFP/ETW testing.


## Stage 1 follow-up: exposed Tauri commands (2026-10-27-04)

**High / fixed in source — biometric unlock bypass:** `src-tauri/src/main.rs` registered `unlock_kdbx_on_biometric_authentication` even though it called the shared-core re-unlock routine without performing Windows Hello or other biometric verification. The frontend's active event flow was already using `unlock_kdbx_with_biometric` (see `src-cljs/main/onekeepass/frontend/events/open_db_form.cljs`), which performs verification and unlock together inside Rust. Revision 2026-10-27-04 removes the old command and its obsolete frontend wrapper, and also removes the deprecated standalone `authenticate_with_biometric` renderer command. The unified authenticated unlock stays registered.

**High / fixed in source — arbitrary local read via obsolete SVG command:** `svg_file` was registered as a Tauri command. Its implementation joined a renderer-supplied `name` to the resource directory, followed by `read_to_string(...).unwrap()`, with no validation of traversal, absolute paths or symlinks. This was identified in source review; exploitation was not demonstrated. The command and unused frontend wrapper are removed; local SVG icons continue through the separate `load_custom_svg_icons` API. The intentionally user-selected SSH key file reader remains enabled and bounded to 256 KiB.

**Regression guard:** the Windows portable build script rejects reintroduction of the three deprecated command registrations and verifies that `unlock_kdbx_with_biometric` remains available.

**Dependency scanner:** the GitHub Actions security audit now also runs after changes to the backend command registrations, SSH agent, core Cargo manifest and Tauri capabilities, not only lockfiles. The advisory scan still archives findings even when scanners report vulnerable packages; a green Actions result does not mean zero advisories.

**Required acceptance:** Windows Hello unlock must display system verification; Cancel must not unlock. A correct verification must unlock with the previously stored session key. Wrong master password must not unlock. Normal key-file/password workflows, KiTTY/WinSCP, local SVG icons and KDBX save/reopen must continue functioning. Run scanner and inspect its reports. CI compilation alone is insufficient.

**Remaining sensitive-data lifecycle issues:** externally opened attachments use decrypted temporary files in `%TEMP%\\okp_cache`, swept on normal exit/startup but not assured after abnormal termination or while an external editor retains the file. Clipboard data and OS clipboard history, session-key plaintext returned by `KeyStoreService`, log/backup NTFS ACLs, and the RSA advisory remain open. Avoid claiming secure deletion from SSD storage.

## Stage 1: decrypted-attachment lifecycle and dependency patches (2026-10-27-05)

The desktop tracks decrypted attachment viewer files separately for each unlocked database and, after a successful lock or close, attempts to delete only that database's temporary files. The OS suspend lock path makes the same cleanup attempt. Failed deletes (typically an editor holding the file open on Windows) are logged and retained for a later retry. Files left by crashes remain subject to the existing startup sweep. **Deletion is best-effort and not cryptographic erasure.** Explicit `Save As` output is not touched.

Redundant Tauri capability defaults `shell:default` and `process:default` are removed. Explicit user-requested URL/file open and app exit remain enabled through `shell:allow-open` and `process:allow-exit`. The portable build rejects reintroducing those defaults and the previously removed unsafe `shell:allow-execute`.

Fresh October 27 advisory scan run [#40563997547](https://github.com/Hiltronix/OneKeePass-desktop/actions/runs/40563997547) reported **six desktop Rust advisory/package records**: `crypto-bigint` 0.5.5 + 0.7.5 (`RUSTSEC-2026-0387`), `quick-xml` 0.39.4 (`RUSTSEC-2026-0194`/`0195`, Linux-specific dependency), and `rsa` 0.9.10 + 0.10.0-rc.18 (`RUSTSEC-2023-0071`); the legacy unshipped browser proxy had one `bytes` advisory. Yarn reported zero vulnerabilities. The controlled lockfile remediation workflow now attempts only a same-series patch to `crypto-bigint 0.7.6`, retaining 0.5.5 for separate transitive dependency analysis. No patch is available for the RSA Marvin advisory as of September 2026.

**Windows acceptance:** with disposable databases A and B, open external-viewer attachments from both, lock A and verify only A's temporary file is removed, then lock B; verify normal app quit cleanup, and document a file remaining open in an editor. Test Windows Hello unlock and denial, entry URL opening, normal Quit, local SVG icons and SSH agent. Run `cargo audit` again on the new lockfile and review results. Do not assert every temporary file is deleted while an external viewer retains a handle, and do not assert that replacing references ensures secure SSD deletion.

## Local-image-only shared core (2026-10-27-06)

Shared-core commit [`68d4a6d`](https://github.com/Hiltronix/OneKeePass-onekeepass-core/commit/68d4a6d21930503566479b55db6a66d93b12e200) separates `favicon-local` (ICO/PNG/JPEG normalization) from the original network-capable `favicon` feature. Desktop now requests `favicon-local` only while keeping local file icon imports; the downloader is gated behind the original feature for other clients. The controlled lockfile regeneration workflow must update `Cargo.lock` for the new pinned core source.

**Limit:** This does not remove other network-capable transitive libraries such as `reqwest_dav` or SFTP/WebDAV transport code; Windows guards and runtime isolation tests remain independently necessary. Gate release on core feature tests, Windows compilation against the refreshed lockfile, local icon import, KDBX save/reopen and SSH agent signing.


## Windows Hello removal and master-password attempt handling — 2026-10-27-07

**Windows Hello completely disabled in the Windows build.** The `UserConsentVerifier` WinRT implementation is removed, biometric availability is always `None` on Windows, and the fused `unlock_kdbx_with_biometric` endpoint is compiled and registered **only on macOS**. The ClojureScript UI no longer recognizes `WindowsHello` and explicitly normalizes the biometric-availability state to `None` on Windows. Windows lock/re-unlock continues through master password and optional key file, with no PIN, face or fingerprint alternative. Touch ID/Face ID on macOS remain intact. Removed WinRT `Foundation` and `Security_Credentials_UI` Cargo feature requests; retained Win32 Credential Manager because the encrypted in-memory KDBX lock still requires its ephemeral session key for restoration **after master-password validation**. The Windows build script checks the platform-gated entry point and absence of Hello symbols.

**Failed-password retry code review:** The pinned shared core's `db_service::unlock_kdbx` immediately returns `HeaderHmacHashCheckFailed` if `compare_key(password, key_file_name)` fails. It has no counter, delay, cooldown or lockout; the initial database-opening path likewise has no application-level failed-attempt limiter in the examined code. None is added here. A GUI lockout cannot prevent offline guessing against a stolen KDBX; KDBX key-derivation cost is a separate protection. Any interactive backoff should be specified and tested separately for restart behavior, denial-of-service risk, and compatibility with master-password recovery.

**Windows runtime acceptance:** inspect About revision, lock then unlock a disposable database using a correct password and optional key file, verify a wrong password does not unlock, verify no PIN/Windows Hello icon or dialog, confirm SSH Agent keys are restored only after correct unlock, and exercise KDBX open/save and local icons. Windows compile alone does not prove successful authentication flow.
