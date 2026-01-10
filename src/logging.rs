use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[cfg(test)]
use tracing::Level;

/// Initialize the logging system
///
/// Uses the RUST_LOG environment variable to control log levels.
/// If verbose is true, sets the default level to DEBUG.
/// Otherwise, defaults to WARN for external crates and INFO for schalentier.
pub fn init(verbose: bool) {
    let default_filter = if verbose {
        "debug,hyper=warn,reqwest=warn"
    } else {
        "schalentier=info,warn"
    };

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

    let subscriber = tracing_subscriber::registry().with(filter).with(
        fmt::layer()
            .with_target(verbose)
            .with_thread_ids(false)
            .with_thread_names(false)
            .with_file(verbose)
            .with_line_number(verbose)
            .with_ansi(true)
            .compact(),
    );

    subscriber.init();
}

/// Initialize logging for tests (only logs errors, no timestamp)
#[cfg(test)]
pub fn init_test() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(Level::ERROR)
        .with_test_writer()
        .try_init();
}

#[cfg(test)]
mod tests {
    use tracing::{debug, error, info, warn};

    #[test]
    fn test_log_levels() {
        // Just verify the logging macros compile correctly
        // We can't easily test actual output without capturing stdout
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_test_writer()
            .try_init();

        debug!("Debug message");
        info!("Info message");
        warn!("Warning message");
        error!("Error message");
    }
}
