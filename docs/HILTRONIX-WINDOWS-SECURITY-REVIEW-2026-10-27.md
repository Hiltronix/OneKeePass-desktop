# Hiltronix Windows desktop security review — 2026-10-27

Scope: `Hiltronix/OneKeePass-desktop` Windows x64 portable release, inspected at
commit `34ff35ecbd81a8c25daa1e805d666c37daf27c1c` (revision 2026-10-27-02).
This review covers two previously open findings: Windows network isolation and
Windows OpenSSH-agent named-pipe access controls. It does **not** include a live
Windows packet capture or another-account connection test. A green CI build
is evidence of compilability, not a passing runtime security audit.

## N-01 — Outbound network traffic: source-level findings

| Component | Static assessment |
|---|---|
| GUI content security policy | Production `connect-src ipc: http://ipc.localhost` prevents ordinary frontend cross-origin fetch, subject to Chromium/Tauri enforcement |
| WebView2 startup | `tauri.conf.json` sets `--disable-background-networking`, `--disable-component-update`, `--disable-sync`, and `--proxy-server=http://127.0.0.1:9`. These are browser configuration measures, **not** an OS network restriction or proof of no connection attempts |
| Automatic updater | `src-tauri/src/updater.rs` still exists in the repository, but `main.rs` does not compile/register it; desktop `Cargo.toml` no longer has the direct reqwest updater dependency. The existing Help menu Check for Updates displays a disabled-function dialog |
| Remote databases | `src-tauri/src/remote_storage/mod.rs` rejects Windows SFTP/WebDAV operations before network transport; related Windows command entry points are guarded |
| Browser integration | Windows code rejects the native-message bridge; offline Windows packaging clears `externalBin` and excludes the browser-proxy executable |
| Intentional external URL opening | Still supported and intentionally launches the chosen external browser. That browser's network traffic is not the desktop executable's own traffic |
| Remaining uncertainty | Windows Rust dependencies retain network-capable code/feature flags; OS/WebView2/third-party behavior can generate short-lived DNS, TLS or other traffic. Static controls do not prove runtime absence |

**N-01 conclusion:** Windows application-owned network activity has not been
measured in a runtime trace. **NOT VERIFIED**, despite several source-level
network call sites being disabled. On a Windows test host, capture process-
attributed connections for `OneKeePass.exe` and `msedgewebview2.exe` while
launching, unlocking, editing, saving, locking, changing SSH agent state and
opening an entry URL. Capture DNS/UDP and short-lived TCP connections using
WFP/ETW, not solely periodic Get-NetTCPConnection polling.

## P-01 — OpenSSH named-pipe permissions: source-level findings

Current Windows path: `src-tauri/src/ssh_agent/pipe.rs` calls
`ssh_agent_lib::agent::NamedPipeListener::bind("\\\\.\\pipe\\openssh-ssh-agent")`.
The app does not pass a custom security descriptor or enforce a same-user
token on the pipe client. The Pageant alternative checks the file-mapping
owner SID, but that check **does not** protect OpenSSH named-pipe requests.

Microsoft documents that a default named-pipe descriptor grants full
control to SYSTEM, administrators and the creator owner, and additionally
read rights to Everyone and ANONYMOUS. Whether those ACEs allow complete
bidirectional agent communication depends on requested access and effective
permissions. We must inspect the **actual** DACL and test from a second,
non-administrator Windows account before asserting an exploitable signing
path or claiming strict same-user isolation.

The SSH Agent's per-key `Require Confirmation` control is an important
independent safeguard: requests can be denied or time out. It does not
prevent another process from connecting, listing identities or attempting
requests.

**P-01 conclusion:** Application-specific named-pipe access restriction is
**ABSENT**. This is a real hardening gap. Effective unauthorized signing
access is **NOT YET DEMONSTRATED**. Corrective work should create every
pipe instance with an explicit same-user SYSTEM-friendly DACL and reject
remote clients, or verify client impersonation SIDs on each accepted
connection, while preserving OpenSSH client compatibility. Replacing or
wrapping the current ssh-agent-lib listener requires a focused Windows
implementation and regression test; do not assume adjusting the initial
listener once secures all subsequently created instances.

Microsoft reference:
https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights

## Windows audit helper

A new read-only script, `scripts/Audit-WindowsSecurity.ps1`, records the
pipe's actual SDDL/ACE list and samples TCP endpoints owned by the running
OneKeePass process and its observed child processes. It **does not** modify
the firewall, Windows security policy, KDBX databases or SSH keys.

Use Windows PowerShell from the repository checkout:

```powershell
.\scripts\Audit-WindowsSecurity.ps1 -Seconds 120
```

The script writes `pipe-acl.json`, `tcp-samples.csv`, `processes.csv`
and `summary.json` to a new directory in `%TEMP%`. It prints that path.
Inspect those reports before sharing: they contain your Windows account SID,
local paths and observed IP addresses. The script deliberately marks both
conclusions **INCONCLUSIVE** until its output is reviewed and the unsampled
network and separate-account negative tests are completed.

### Runtime acceptance criteria

- Repeat the network test with SSH Agent on/off and WebView2 child processes
  visible; use WFP/ETW event capture to detect DNS/UDP and short-lived TCP.
- Inspect the actual pipe DACL at startup and after reconnect; check for
  Everyone/Anonymous/Users read/write rights, not just the owner label.
- With a disposable SSH identity, verify the current user can enumerate and
  sign through OpenSSH and that KiTTY/WinSCP still work via Pageant.
- From a *different unprivileged Windows account*, verify pipe access and
  private-key signing both fail (or enforce confirmation deny with no ability
  to sign). Do not use a production SSH private key.
- Only mark either audit complete after recording the observed results.

Audit status: **SOURCE REVIEW COMPLETED; WINDOWS RUNTIME CHECKS OPEN.**


## User Windows runtime observation — 2026-10-27

An initial two-minute run on Windows 10, reported by the user, observed 7 related processes (OneKeePass.exe plus six msedgewebview2.exe descendants) and **zero sampled TCP records**. This is favorable evidence from TCP polling, not a complete absence-of-network proof (misses UDP, DNS and short connections).

The same run could not retrieve the pipe ACL: `GetSecurityInfo` returned Win32 error `87` (invalid parameter). The script incorrectly passed `SE_FILE_OBJECT=1` for a named-pipe handle. The audit helper has been corrected to use `SE_KERNEL_OBJECT=6`. The initial `BroadAccessDetected: False` value is **not evidence of a restrictive DACL**; no SDDL or ACEs were actually retrieved. Recheck using the corrected helper; a five-second run is sufficient to inspect the pipe DACL.

## P-01 remediation candidate — revision 2026-10-27-03

After the live ACL inspection showed `SYSTEM`, `Administrators`, and the current user with full control, and `Everyone`/`Anonymous` with read access, the Windows OpenSSH transport was changed to **create every pipe instance with an explicit protected security descriptor**:

```text
D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;<current account SID>)
```

The SID is derived at runtime from the OneKeePass process token. Every instance, including replacements created after clients connect, uses the same descriptor. The listener also explicitly rejects remote pipe clients. Agent key storage, signing approval, timeout handling, Pageant interface and standard pipe name are unchanged.

**Validation is pending:** a Windows/MSVC compile, the actual pipe ACL inspection (`QueryResult: 0`, no Everyone or Anonymous ACE), `ssh-add -l`, SSH authentication, and an unprivileged second-account access test. A change to the source is not proof of effective isolation. Previous 2026-10-27-02 binary still exposes the default DACL.


## P-01 Windows runtime verification — 2026-10-27 (revision 2026-10-27-03)

The user installed the revised Windows build and provided an actual live named-pipe ACL inspection, rather than relying on source code alone:

- Windows security query returned `QueryResult: 0` with no query error.
- SDDL DACL: `D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;<current account SID>)`.
- The only allow ACEs were SYSTEM (`S-1-5-18`), Builtin Administrators (`S-1-5-32-544`), and the current user (`0x001F01FF`, full control). No Everyone (`S-1-1-0`) or Anonymous (`S-1-5-7`) ACEs were present.
- `BroadAccessDetected: false`.
- `ssh-add -l` successfully enumerated two ED25519 identities.
- The user confirmed **both KiTTY and WinSCP successfully authenticate with OKP keys** after the ACL change.
- The accompanying 5-second TCP sample observed zero TCP connections.

**Conclusion:** The restricted DACL, normal OpenSSH identity enumeration, and KiTTY/WinSCP compatibility are **runtime validated**. A deliberate cross-account negative-access test has **not** yet been performed; elevated Administrators and SYSTEM intentionally retain access. Network isolation remains only *partially* tested because periodic TCP sampling omits transient connections and UDP/DNS.


## Stage 1 security command-surface review — revision 2026-10-27-04

Audit identified two renderer-callable privileged commands requiring removal: an unauthenticated legacy biometric unlock, and an obsolete caller-supplied SVG path reader without containment checks. Both commands, corresponding deprecated frontend wrappers, and the unneeded two-step biometric verification command are removed in this revision. The existing fused `unlock_kdbx_with_biometric` path remains. A build-level regression check rejects re-registering these commands.

Residual network-capable features in core (`favicon`, `reqwest_dav`) remain linked despite Windows network guards; feature pruning cannot be represented as a no-risk refactor without preserving local-icon import and auditing shared-core Cargo features. No network-facing runtime exploit of the RSA advisory was found in the reviewed Windows agent: its RSA signing is through restricted local IPC and confirmation controls. `RUSTSEC-2023-0071` remains present and **unpatched** per the September 2026 RustSec advisory; do not interpret local-only use as a universal security fix.

Validation: Windows build, advisory rescan, Windows Hello rejection/success, local icons, SSH agent regression; external temporary attachment lifecycle and WFP/ETW traffic tracing still pending.

## Revision 2026-10-27-05: sensitive-file and capability hardening

- Per-database decrypted viewer attachment tracking enables cleanup on lock, close and OS suspend without deleting another unlocked database's attachments.
- Windows locked files may resist deletion; those failures remain recorded for retry and the usual startup/exit sweep. The feature cannot guarantee secure deletion of underlying SSD blocks, cloud backups or editors' copies.
- Removes redundant `shell:default`/`process:default` Tauri permissions; explicit `shell:allow-open` and `process:allow-exit` remain so local user actions keep working.
- Rust advisory scan identified freshly disclosed `RUSTSEC-2026-0387` in `crypto-bigint` 0.5.5 and 0.7.5; patched 0.7.6 is targeted by the controlled lockfile-update workflow. The 0.5.x chain is not blindly upgraded across incompatible releases.
