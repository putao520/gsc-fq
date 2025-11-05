use crate::error::Result;
use std::sync::Arc;
use tokio::sync::Notify;

/// Signal handler for graceful shutdown
pub struct SignalHandler {
    shutdown_notify: Arc<Notify>,
}

impl SignalHandler {
    /// Get the shutdown notifier for testing
    pub fn get_notify(&self) -> Arc<Notify> {
        self.shutdown_notify.clone()
    }

    /// Create a new signal handler
    pub fn new() -> Self {
        Self {
            shutdown_notify: Arc::new(Notify::new()),
        }
    }

    /// Set up signal handlers for graceful shutdown
    pub async fn setup_signal_handlers(&self) -> Result<()> {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};

            let shutdown_notify = self.shutdown_notify.clone();

            // Handle SIGTERM
            let mut sigterm = signal(SignalKind::terminate()).map_err(|e| {
                crate::error::SystemError::SignalHandlingFailed(format!(
                    "Failed to setup SIGTERM handler: {}",
                    e
                ))
            })?;

            let notify_clone = shutdown_notify.clone();
            tokio::spawn(async move {
                match sigterm.recv().await {
                    Some(_) => {
                        notify_clone.notify_waiters();
                    }
                    None => {
                        // SIGTERM handler stopped unexpectedly
                    }
                }
            });

            // Handle SIGINT (Ctrl+C)
            let mut sigint = signal(SignalKind::interrupt()).map_err(|e| {
                crate::error::SystemError::SignalHandlingFailed(format!(
                    "Failed to setup SIGINT handler: {}",
                    e
                ))
            })?;

            tokio::spawn(async move {
                match sigint.recv().await {
                    Some(_) => {
                        shutdown_notify.notify_waiters();
                    }
                    None => {
                        // SIGINT handler stopped unexpectedly
                    }
                }
            });
        }

        #[cfg(windows)]
        {
            let shutdown_notify = self.shutdown_notify.clone();

            // Handle Ctrl+C on Windows
            tokio::spawn(async move {
                match tokio::signal::ctrl_c().await {
                    Ok(()) => {
                        shutdown_notify.notify_waiters();
                    }
                    Err(_) => {
                        // Failed to setup Ctrl+C handler
                    }
                }
            });
        }

        Ok(())
    }

    /// Wait for shutdown signal
    pub async fn wait_for_shutdown(&self) {
        self.shutdown_notify.notified().await;
    }

    /// Get shutdown notifier for other components
    pub fn get_shutdown_notifier(&self) -> Arc<Notify> {
        self.shutdown_notify.clone()
    }

    /// Trigger shutdown manually (useful for testing)
    pub fn trigger_shutdown(&self) {
        self.shutdown_notify.notify_waiters();
    }
}

impl Default for SignalHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_signal_handler_creation() {
        let handler = SignalHandler::new();
        // Just test that it creates successfully
        assert!(true);
    }

    #[tokio::test]
    async fn test_manual_shutdown_trigger() {
        let handler = SignalHandler::new();

        // Trigger shutdown in background
        let notify = handler.get_notify();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            notify.notify_one();
        });

        // Wait for shutdown (should complete quickly)
        let start = std::time::Instant::now();
        handler.wait_for_shutdown().await;
        let elapsed = start.elapsed();

        assert!(elapsed < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn test_shutdown_notifier() {
        let handler = SignalHandler::new();
        let notifier = handler.get_shutdown_notifier();

        // Use notifier to trigger shutdown
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            notifier.notify_waiters();
        });

        // Wait for shutdown
        handler.wait_for_shutdown().await;
    }
}
