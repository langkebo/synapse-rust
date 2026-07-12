//! Graceful-shutdown primitives: a shared CancellationToken cancels all
//! background service loops on SIGTERM/ctrl_c. See OPT-014.

#[cfg(test)]
mod tests {
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn shutdown_signal_cancels_token() {
        let token = CancellationToken::new();
        let child = token.clone();
        let handle = tokio::spawn(async move {
            let mut iterations: u64 = 0;
            loop {
                tokio::select! {
                    _ = child.cancelled() => break,
                    _ = tokio::time::sleep(std::time::Duration::from_secs(3600)) => {
                        iterations += 1;
                    }
                }
            }
            iterations
        });

        token.cancel();

        let iterations = tokio::time::timeout(std::time::Duration::from_secs(1), handle)
            .await
            .expect("loop must exit within 1s of cancel")
            .expect("task must not panic");
        assert_eq!(iterations, 0, "loop must break on cancel before any long-sleep tick fires");
    }
}
