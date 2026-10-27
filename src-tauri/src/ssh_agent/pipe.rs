// Windows OpenSSH named-pipe transport for the SSH agent.
//
// Tokio's Windows named-pipe implementation needs an active I/O reactor.
// Do not schedule binding on the shared Tauri runtime and synchronously wait
// for its acknowledgement: when start() is itself called on that runtime, the
// wait can starve the task until it times out (observed in Windows logs).
//
// A dedicated OS thread owns a current-thread Tokio reactor for the lifetime
// of the named-pipe listener. The caller receives startup success only after
// the listener has actually bound; shutdown signals that thread and stop()
// joins it before a new agent can bind the same pipe.

use std::ffi::c_void;
use std::io;
use std::ptr::{null_mut, read_unaligned};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use ssh_agent_lib::agent::{Agent, ListeningSocket, Session};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::Notify;
use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, PSID};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, TokenUser, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
    TOKEN_QUERY, TOKEN_USER,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use super::session::AgentSession;
use super::store::SshAgentStore;

// The standard name used by Windows OpenSSH and Git-for-Windows clients.
pub(crate) const PIPE_PATH: &str = r"\\.\pipe\openssh-ssh-agent";

// Create an explicit, protected DACL for the *current account*, SYSTEM, and
// Administrators. Never use the Windows default named-pipe DACL: it gives
// Everyone and Anonymous read rights. The account SID is obtained from the
// process token at runtime; no SID, usernames or secrets are hard-coded.
fn account_sid() -> io::Result<String> {
    unsafe {
        let mut token: HANDLE = 0;
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(io::Error::last_os_error());
        }
        let result = (|| {
            let mut required = 0_u32;
            GetTokenInformation(token, TokenUser, null_mut(), 0, &mut required);
            if required == 0 {
                return Err(io::Error::last_os_error());
            }

            let mut buf = vec![0_u8; required as usize];
            if GetTokenInformation(
                token,
                TokenUser,
                buf.as_mut_ptr() as *mut c_void,
                required,
                &mut required,
            ) == 0 {
                return Err(io::Error::last_os_error());
            }

            // TOKEN_USER contains a pointer into the allocated token buffer.
            // read_unaligned avoids assuming alignment for Vec<u8>.
            let owner: TOKEN_USER = read_unaligned(buf.as_ptr() as *const TOKEN_USER);
            let mut sid_text: *mut u16 = null_mut();
            if ConvertSidToStringSidW(owner.User.Sid as PSID, &mut sid_text) == 0 {
                return Err(io::Error::last_os_error());
            }
            let mut length = 0;
            while *sid_text.add(length) != 0 {
                length += 1;
            }
            let sid = String::from_utf16_lossy(std::slice::from_raw_parts(sid_text, length));
            LocalFree(sid_text as *mut c_void);
            Ok(sid)
        })();
        CloseHandle(token);
        result
    }
}

// A self-relative Win32 security descriptor returned by LocalAlloc through the
// SDDL conversion API. Its lifetime must extend through CreateNamedPipeW.
struct PipeSecurityDescriptor(PSECURITY_DESCRIPTOR);
impl Drop for PipeSecurityDescriptor {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_null() {
                LocalFree(self.0 as *mut c_void);
            }
        }
    }
}

fn create_secured_pipe(path: &str, sddl: &[u16], first: bool) -> io::Result<NamedPipeServer> {
    unsafe {
        let mut raw: PSECURITY_DESCRIPTOR = null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1, // SDDL_REVISION_1
            &mut raw,
            null_mut(),
        ) == 0 {
            return Err(io::Error::last_os_error());
        }
        let descriptor = PipeSecurityDescriptor(raw);
        let attrs = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(
                path,
                &attrs as *const SECURITY_ATTRIBUTES as *mut c_void,
            )
    }
}

// ssh-agent-lib's standard listener creates a fresh pipe on every accept using
// *default* Windows security. This equivalent adapter supplies the restricted
// security descriptor for the FIRST instance AND EVERY subsequent instance.
#[derive(Debug)]
struct RestrictedPipeListener {
    waiting: NamedPipeServer,
    security_sddl: Vec<u16>,
}

impl RestrictedPipeListener {
    fn bind(path: &str) -> io::Result<Self> {
        let sid = account_sid()?;
        let sddl = format!("D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;{sid})");
        let security_sddl: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();
        let waiting = create_secured_pipe(path, &security_sddl, true)?;
        Ok(Self {
            waiting,
            security_sddl,
        })
    }
}

#[ssh_agent_lib::async_trait]
impl ListeningSocket for RestrictedPipeListener {
    type Stream = NamedPipeServer;

    async fn accept(&mut self) -> io::Result<Self::Stream> {
        self.waiting.connect().await?;
        // New instance is created with the SAME DACL; never expose a default
        // instance to a waiting or reconnecting client.
        let next = create_secured_pipe(PIPE_PATH, &self.security_sddl, false)?;
        Ok(std::mem::replace(&mut self.waiting, next))
    }
}

// Reuse the existing agent protocol and confirmation flow unchanged.
impl Agent<RestrictedPipeListener> for AgentSession {
    fn new_session(&mut self, _socket: &NamedPipeServer) -> impl Session {
        self.clone()
    }
}


// Return the owning worker handle on success. It must be joined after shutdown
// so that immediate stop/restart cannot race with the previous bound listener.
pub(crate) fn bind_and_spawn(
    store: Arc<RwLock<SshAgentStore>>,
    shutdown: Arc<Notify>,
) -> Result<JoinHandle<()>, String> {
    let (bind_tx, bind_rx) = std::sync::mpsc::sync_channel::<Result<(), String>>(1);

    let worker = thread::Builder::new()
        .name("onekeepass-ssh-pipe".to_string())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = bind_tx.send(Err(format!(
                        "failed to start OpenSSH named-pipe I/O runtime: {e}"
                    )));
                    return;
                }
            };

            runtime.block_on(async move {
                let listener = match RestrictedPipeListener::bind(PIPE_PATH) {
                    Ok(listener) => listener,
                    Err(e) => {
                        let _ = bind_tx.send(Err(format!(
                            "failed to bind {PIPE_PATH}: {e}. Another SSH agent                              may already own this pipe"
                        )));
                        return;
                    }
                };

                // If startup has timed out and the receiver has been dropped,
                // abandon this listener immediately: never leave an orphan pipe
                // running after the caller reported failure.
                if bind_tx.send(Ok(())).is_err() {
                    return;
                }

                log::info!("SSH agent listening on {} (restricted same-user DACL)", PIPE_PATH);
                let session = AgentSession::new(store);
                tokio::select! {
                    result = ssh_agent_lib::agent::listen(listener, session) => {
                        if let Err(e) = result {
                            log::error!(
                                "SSH agent named-pipe listener ended with error: {:?}", e
                            );
                        }
                    }
                    _ = shutdown.notified() => {
                        log::info!("SSH agent named-pipe listener received shutdown signal");
                    }
                }
            });
        })
        .map_err(|e| format!("failed to create OpenSSH named-pipe worker: {e}"))?;

    // Waiting here is safe even if the caller runs on a Tauri worker:
    // binding is on a *separate OS thread with its own Tokio runtime*.
    let started = bind_rx
        .recv_timeout(Duration::from_secs(10))
        .map_err(|e| format!("timed out waiting for {PIPE_PATH} bind result: {e}"))?;

    match started {
        Ok(()) => Ok(worker),
        Err(e) => {
            // This worker has already reported an error and is exiting.
            let _ = worker.join();
            Err(e)
        }
    }
}
