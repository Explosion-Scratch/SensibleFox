use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Weighted step in an install pipeline.
pub struct Step {
    pub title: &'static str,
    pub weight: u32,
}

/// Unified progress reporter. Drives the CLI bar (interactive runs) and
/// the `/tmp/sensiblefox-install.status` file (PKG / applet runs) from a
/// single source of truth.
pub struct Progress {
    status_file: Option<PathBuf>,
    steps: Vec<Step>,
    cumulative: Vec<u32>,
    total: u32,
    current: Mutex<usize>,
    pb: Option<ProgressBar>,
    quiet: bool,
}

impl Progress {
    pub fn new(status_file: Option<PathBuf>, steps: Vec<Step>) -> Self {
        let total: u32 = steps.iter().map(|s| s.weight).sum::<u32>().max(1);
        let mut cumulative = Vec::with_capacity(steps.len() + 1);
        let mut acc = 0u32;
        cumulative.push(0);
        for s in &steps {
            acc += s.weight;
            cumulative.push(acc);
        }
        let quiet = status_file.is_some();
        let pb = if quiet {
            None
        } else {
            let pb = ProgressBar::new(100);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("  {msg}\n  [{bar:40.cyan/dim}] {pos}%")
                    .unwrap()
                    .progress_chars("█▓░"),
            );
            pb.set_position(0);
            Some(pb)
        };
        Progress {
            status_file,
            steps,
            cumulative,
            total,
            current: Mutex::new(usize::MAX),
            pb,
            quiet,
        }
    }

    pub fn is_quiet(&self) -> bool {
        self.quiet
    }

    /// Begin a new step. Advances the global bar to the step's base position.
    pub fn step(&self, index: usize, detail: &str) {
        let mut cur = self.current.lock().unwrap();
        *cur = index;
        drop(cur);
        let title = self.steps.get(index).map(|s| s.title).unwrap_or("");
        let base = self.percent_at(index, 0.0);
        if let Some(ref pb) = self.pb {
            pb.set_position(base as u64);
            pb.set_message(format!("{} — {}", style(title).bold(), detail));
        } else if !self.quiet {
            println!("{} {}", style("→").blue().bold(), style(title).bold());
        }
        self.write(title, detail, base);
    }

    /// Update detail / sub-fraction for the current step (0.0..=1.0).
    pub fn sub(&self, fraction: f64, detail: &str) {
        let idx = *self.current.lock().unwrap();
        if idx == usize::MAX {
            return;
        }
        let title = self.steps.get(idx).map(|s| s.title).unwrap_or("");
        let pct = self.percent_at(idx, fraction.clamp(0.0, 1.0));
        if let Some(ref pb) = self.pb {
            pb.set_position(pct as u64);
            pb.set_message(format!("{} — {}", style(title).bold(), detail));
        }
        self.write(title, detail, pct);
    }

    /// Same as `sub` but signals indeterminate progress to the applet.
    pub fn indeterminate(&self, detail: &str) {
        let idx = *self.current.lock().unwrap();
        let title = self
            .steps
            .get(idx.min(self.steps.len().saturating_sub(1)))
            .map(|s| s.title)
            .unwrap_or("");
        if let Some(ref pb) = self.pb {
            pb.set_message(format!("{} — {}", style(title).bold(), detail));
        }
        if let Some(ref sf) = self.status_file {
            write_status_file(sf, "progress", title, detail, -1, -1);
        }
    }

    pub fn finish(&self) {
        if let Some(ref pb) = self.pb {
            pb.set_position(100);
            pb.finish_and_clear();
        }
        if let Some(ref sf) = self.status_file {
            write_status_file(
                sf,
                "done",
                "SensibleFox installed",
                "Firefox is ready to launch.",
                100,
                100,
            );
        }
    }

    pub fn fail(&self, title: &str, detail: &str) {
        if let Some(ref pb) = self.pb {
            pb.finish_and_clear();
        }
        if let Some(ref sf) = self.status_file {
            write_status_file(sf, "error", title, detail, 0, 100);
        }
    }

    fn percent_at(&self, index: usize, fraction: f64) -> u32 {
        let base = *self.cumulative.get(index).unwrap_or(&0) as f64;
        let span = self
            .steps
            .get(index)
            .map(|s| s.weight as f64)
            .unwrap_or(0.0);
        let p = (base + span * fraction) * 100.0 / self.total as f64;
        p.round().clamp(0.0, 100.0) as u32
    }

    fn write(&self, title: &str, detail: &str, progress: u32) {
        if let Some(ref sf) = self.status_file {
            write_status_file(sf, "progress", title, detail, progress as i64, 100);
        }
    }
}

fn write_status_file(path: &Path, step: &str, title: &str, detail: &str, progress: i64, total: i64) {
    // Write directly (not via temp+rename) so this works regardless of who
    // originally created the file. The status file lives in /tmp which has
    // a sticky bit; a non-owner rename across the sticky bit would fail.
    // The applet tolerates partial reads — it just retries on the next poll.
    let content = format!(
        "step={step}\ntitle={title}\ndetail={detail}\nprogress={progress}\ntotal={total}\n"
    );
    let _ = std::fs::write(path, content);
}
