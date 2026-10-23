//! A spinner on stderr for operations that take a while, shown only on a
//! terminal and never with `--json` or `--quiet`.

use std::time::Duration;

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

/// A running spinner; a no-op when progress is not shown.
pub struct Spinner(Option<ProgressBar>);

impl Spinner {
    /// Start a spinner saying `message`, when `shown`.
    pub fn start(shown: bool, message: impl Into<String>) -> Spinner {
        if !shown {
            return Spinner(None);
        }
        let bar = ProgressBar::with_draw_target(None, ProgressDrawTarget::stderr());
        bar.set_style(
            ProgressStyle::with_template("{spinner:.cyan} {msg} {elapsed:.dim}")
                .unwrap_or_else(|_| ProgressStyle::default_spinner())
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", "✓"]),
        );
        bar.set_message(message.into());
        bar.enable_steady_tick(Duration::from_millis(80));
        Spinner(Some(bar))
    }

    /// Change the message.
    pub fn set(&self, message: impl Into<String>) {
        if let Some(bar) = &self.0 {
            bar.set_message(message.into());
        }
    }

    /// Remove the spinner.
    pub fn finish(self) {
        if let Some(bar) = self.0 {
            bar.finish_and_clear();
        }
    }
}
