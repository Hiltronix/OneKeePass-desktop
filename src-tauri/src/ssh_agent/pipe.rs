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

use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tokio::sync::Notify;
use ssh_agent_lib::agent::NamedPipeListener;

use super::session::AgentSession;
use super::store::SshAgentStore;

// The standard name used by Windows OpenSSH and Git-for-Windows clients.
pub(crate) const PIPE_PATH: &str = r"\\.\pipe\openssh-ssh-agent";

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
                let listener = match NamedPipeListener::bind(PIPE_PATH) {
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

                log::info!("SSH agent listening on {}", PIPE_PATH);
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
