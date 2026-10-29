# OneKeePass 0.27.0 — Hiltronix Windows Desktop Edition

> **Unofficial customized fork.** This edition is intentionally based on **OneKeePass Desktop 0.27.0** rather than tracking newer upstream releases. The goal is a locally controlled Windows portable password manager: reduce unnecessary network exposure, update selected libraries, correct functional problems, make minor usability improvements, and harden security. It is not an official OneKeePass release or an automatic-updating distribution.

## Purpose and key modifications

- **Version pinning:** Keeps the OneKeePass 0.27.0 application version while assigning a separate date-based **Hiltronix Revision** in *About*. The final tested Windows x64 portable build is **2026-10-28-6**.
- **Offline-first Windows build:** Disables the update-check mechanism, blocks background WebView2 networking/component updates/sync, restricts WebView2 through a loopback-only nonresponsive proxy, and omits the browser native-messaging proxy executable from the portable package. These measures reduce network exposure; they do **not** constitute a firewall or guarantee that every possible network operation is blocked.
- **Local-only icon loading:** Prevents fetching custom icons/favicons from remote URLs; local icon files remain supported.
- **Reduced exposed functions:** Removed unused renderer-callable plaintext XML export functions and wrappers, narrowed Tauri permissions, and removed Windows Hello biometric/PIN code paths from the Windows build.
- **Portable filesystem isolation:** Keeps application data in `onekeepass-data` beside `OneKeePass.exe`; redirects working files, temporary attachment copies, Tauri caches, and the WebView2 profile under the portable `temp` directory. Cleans temporary contents on normal exit and stale contents on next launch while retaining the folder itself.
- **SSH-agent support and hardening:** Windows OpenSSH named-pipe and Pageant compatibility, more restrictive named-pipe permissions and Windows ownership checks, and signing support tested with external SSH clients. The SSH agent remains an *explicit* local feature, not an external network service.
- **Rust dependency maintenance:** Updated selected cryptographic and XML dependencies, including the **crypto-bigint 0.7.10** migration in the custom source; pinned and tested SSH-agent-related library compatibility.
- **Hardening of file/attachment operations:** Added size-bounded local file reads and stricter authorization for temporary attachment-opening paths, including revocation when the database is locked.
- **Security diagnostics:** Added a Windows security-audit helper and a dependency advisory workflow; audits also checked outbound TCP activity and Windows named-pipe access controls. These checks are not a blanket security certification.
- **Search usability:** Added an always-visible toolbar search field with **Ctrl+F**, removed the old saved-input suggestions, and adjusted the search dropdown height.
- **Editing shortcuts and dialogs:** **Ctrl+S** applies changes while editing an entry; added **Escape** cancellation to several dialogs; repaired the Clone Entry text-cursor behavior, and corrected entry-delete confirmation UI regressions.
- **Windows desktop polish:** Preserves main-window geometry, fixes application/taskbar identity, and improves the *About* and *Help* experience.

## Custom build and original credits

**Windows x64 portable download:** [Hiltronix GitHub Releases](https://github.com/Hiltronix/OneKeePass-desktop/releases). Extract the ZIP and run `OneKeePass.exe`. Preserve the hidden `.portable` file and `_up_` resources. This build requires the Microsoft Edge WebView2 Runtime.

**Final tested source:** [revision `2026-10-28-6`, commit `794efd78e700bd955b48f92563161aef49350def`](https://github.com/Hiltronix/OneKeePass-desktop/commit/794efd78e700bd955b48f92563161aef49350def) on [`fix/desktop-dialogs-v6-stage`](https://github.com/Hiltronix/OneKeePass-desktop/tree/fix/desktop-dialogs-v6-stage). The **default `main` branch is not the final Rev 6 Windows build**; choose the custom branch to work from the tested source.

**Original project:** [OneKeePass Desktop](https://github.com/OneKeePass/desktop), by its original developers and contributors. All original authorship, licensing, notices and source-code attribution are preserved. Hiltronix is credited only for the modifications in this fork. The original README below describes the upstream cross-platform application and may include features intentionally changed or disabled in this Windows-specific edition.

---

## Original OneKeePass README

<h1 align="center">
  <img src="./src-tauri/icons/128x128.png" alt="OneKeePass" width="128" />
  <br>
  <div>OneKeePass</div>
</h1>

**OneKeePass** is a cross-platform,open source and secure password manager. It supports the KeePass-compatible database (Kdbx 4.x). It stores and manages all your passwords and other sensitive information in a single encrypted file that can be stored in any location, including private and public cloud solutions. The database is locked with a master key. 

All created database files are encrypted using the best and most secure encryption algorithms currently known (AES-256, ChaCha20).

<h1 align="center">
  <img src="./screenshots/entrycategory-type-allentries.jpg" alt=""  />
  <br>
</h1>

<h1 align="center">
  <img src="./screenshots/entrycategory-type-allentries-okp-dark-mode.jpg" alt=""  />
  <br>
</h1>


## Features
- Simple and Modern UI
- Entries are grouped as Types or Categories or the standard Group tree
- Custom entry type with any set of fields can be created as template and used to create entries
- Any number of custom fields can be added
- Custom fields can be organized as sections
- Upload attachments to an entry
- Basic Auto-Type feature that sends simulated keypresses to other applications in macOS
- Quick database unlock feature using TouchID for macOS and Windows Hello for Windows
- Any number of databases can be created and used
- Optional additional security by using a key file with or without a master password
- Time based One-Time Passwords (TOTP) can added and used
- Supports Light/Dark Theme
- Multi-language support 
- Automatic Database Opening feature
- Cloning/Copying entries
- Diceware Passphrase generator
- Merging of databases
- CSV file import — exports from Bitwarden, 1Password, LastPass, NordPass, Proton Pass, Dashlane, Safari/iCloud Passwords, Firefox and Chrome/Edge are recognised automatically, and any other CSV file can be mapped by hand
- Fully offline 
- Browser extension and integration (alpha version) - [Firefox](https://addons.mozilla.org/en-US/firefox/addon/onekeepass-browser/), [Chrome](https://chromewebstore.google.com/detail/onekeepass-browser/cmdmojmbfcpkloflnjkkdjcflaidangh) and Brave
- Supports password autofill, passkey registration and passkey authentication - OneKeePass-Browser extension is required
- Custom icons support — upload icons from a file or fetch from a URL/favicon; assign to entries and groups
- Direct SFTP and WebDAV remote server support — create or open databases stored on a remote server without any third-party sync tool
- Remote server credentials stored as **SFTP Connection** or **WebDAV Connection** entries inside the database
- Additional built-in entry types — SSH Key, Identity, Passport and Driver License
- Custom fields can be Text, Boolean or Date type (with a date picker for date fields)
- SSH Agent service — store SSH keys as **SSH Key** entries and serve them to SSH clients in Agent Mode (own socket/named pipe) or Client Mode (existing system agent, including OpenSSH and Pageant on Windows)

## Installation and Quick start

You can start using OneKeePass on your Windows, macOS, or Linux computer using the latest pre-compiled binaries from the **[downloads page](https://github.com/OneKeePass/desktop/releases)**

### Mac store

The macOS app can also be downloaded from the Mac App Store:

<a href="https://apps.apple.com/us/app/onekeepass-password-manager/id6764709055">
  <img src="./screenshots/mac-app-store-badge.svg" alt="Download on the Mac App Store" height="48" />
</a>


### Windows Portable version

A portable zip (`OneKeePass_<version>_Windows_x64_Portable.zip`) is available on the downloads page. Unzip it to any writable location - for example a USB drive - and run `OneKeePass.exe`; no installation is needed. All application data (preferences, logs, backups, word lists) is kept in the `onekeepass-data` folder next to the exe, so nothing is left behind on the computer you use it on. It requires the Microsoft Edge WebView2 Evergreen Runtime, which is preinstalled on current Windows 10 and 11 systems. See the `README.txt` inside the zip for details

### OneKeePass-Browser

Browser extensions are available for [Firefox](https://addons.mozilla.org/en-US/firefox/addon/onekeepass-browser/), [Chrome](https://chromewebstore.google.com/detail/onekeepass-browser/cmdmojmbfcpkloflnjkkdjcflaidangh) and [Brave](https://chromewebstore.google.com/detail/onekeepass-browser/cmdmojmbfcpkloflnjkkdjcflaidangh)  at this time

### Mobile apps

OneKeePass is also available for iOS and Android platforms. See [here](https://github.com/OneKeePass/mobile) for details and download

## FAQ

Common **FAQ** about the application can be found [here](./docs/FAQ.md)

## How to build

OneKeePass is developed primarily using the ClojureScript and Rust programming languages and is built with - [Tauri](https://tauri.app/), [React](https://react.dev/), [MUI](https://mui.com/) and many Rust crates

To build your own binary, please follow the instructions [here](./docs/BUILD.md)

## How to Contribute

### Translations

If you want to contribute translation of texts used in this application in your language of choice, please see [OneKeePass Translations](https://github.com/OneKeePass/onekeepass-translations)

### Code
The CONTRIBUTING document and other guidelines for code contributions are not yet ready. Till that time, please feel free to::
- [Report bugs, suggest new features](https://github.com/OneKeePass/desktop/issues)
 
## License
This project is licensed under the GPLv3.
