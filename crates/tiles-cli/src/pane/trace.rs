//! `tiles pane --trace <file>` (ADR-063): one line per event, which
//! `scripts/pane-trace.py` turns into the go or no-go table.
//!
//! Each line is the milliseconds since the command started, the event and
//! its fields as `key=value`:
//!
//! - `start terminal=<name> pixels=<yes|no> cell=<w>x<h> scale=<s>`: the
//!   pane took the screen.
//! - `read bytes=<b> reports=<k>`: input read; `reports` counts mouse
//!   reports. An update a read causes is timed from it.
//! - `begin update=<n> kind=<first|hover|click|resize|scroll>`: an update
//!   starts. The first starts when the command does, at 0.
//! - `draw update=<n> kind=<kind> card=<i>`: card `i` (or the overlay,
//!   `card=overlay`) is drawn and compressed.
//! - `write update=<n> kind=<kind> bytes=<b> cards=<k>`: the update's bytes
//!   are written and flushed.
//! - `rss kb=<k>` and `cpu ms=<total>`: the process's resident memory and
//!   its user and system time so far, sampled once a second.

use std::io::Write as _;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The trace file, or nothing when `--trace` is not given.
#[derive(Debug)]
pub struct Trace {
    file: Option<Mutex<std::fs::File>>,
    start: Instant,
}

impl Trace {
    /// A trace to `path` (created or emptied), timed from `start`; one
    /// that writes nothing when `path` is `None`.
    ///
    /// # Errors
    ///
    /// When the file cannot be created.
    pub fn open(path: Option<&Path>, start: Instant) -> Result<Self, String> {
        let file = path
            .map(|p| {
                std::fs::File::create(p).map_err(|e| {
                    format!(
                        "cannot create the trace {}: {e}",
                        tiles_intent::one_line(&p.display().to_string())
                    )
                })
            })
            .transpose()?;
        Ok(Self {
            file: file.map(Mutex::new),
            start,
        })
    }

    /// Whether lines are written.
    #[must_use]
    pub fn on(&self) -> bool {
        self.file.is_some()
    }

    /// Writes one line: the time, `event` and `fields`.
    pub fn line(&self, event: &str, fields: std::fmt::Arguments<'_>) {
        let Some(file) = &self.file else { return };
        let ms = self.start.elapsed().as_secs_f64() * 1000.0;
        if let Ok(mut f) = file.lock() {
            let _ = writeln!(f, "{ms:.3} {event} {fields}");
        }
    }
}

/// Samples resident memory and CPU time into `trace` every `period` from
/// a thread of its own, until the process ends.
pub fn sample(trace: Arc<Trace>, period: Duration) {
    if !trace.on() {
        return;
    }
    std::thread::spawn(move || {
        loop {
            if let Some((kb, cpu_ms)) = usage() {
                trace.line("rss", format_args!("kb={kb}"));
                trace.line("cpu", format_args!("ms={cpu_ms:.1}"));
            }
            std::thread::sleep(period);
        }
    });
}

/// This process's resident memory in KB and CPU time in milliseconds.
#[cfg(target_os = "linux")]
fn usage() -> Option<(u64, f64)> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    Some((statm_kb(&statm)?, stat_cpu_ms(&stat)?))
}

/// This process's resident memory in KB and CPU time in milliseconds, from
/// `ps`, since macOS keeps them behind calls safe Rust cannot make.
#[cfg(not(target_os = "linux"))]
fn usage() -> Option<(u64, f64)> {
    let out = std::process::Command::new("/bin/ps")
        .args(["-o", "rss=,time=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    ps_line(std::str::from_utf8(&out.stdout).ok()?)
}

/// `ps -o rss=,time=`'s line: KB, then `[[dd-]hh:]mm:ss.cc`.
#[cfg_attr(target_os = "linux", allow(dead_code))]
fn ps_line(line: &str) -> Option<(u64, f64)> {
    let mut words = line.split_whitespace();
    let kb = words.next()?.parse().ok()?;
    let time = words.next()?;
    let (days, clock) = match time.split_once('-') {
        Some((d, rest)) => (d.parse::<f64>().ok()?, rest),
        None => (0.0, time),
    };
    let mut secs = 0.0;
    for part in clock.split(':') {
        secs = secs * 60.0 + part.parse::<f64>().ok()?;
    }
    Some((kb, (days * 86_400.0 + secs) * 1000.0))
}

/// `/proc/self/statm`'s resident pages as KB, at 4 KB a page.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn statm_kb(statm: &str) -> Option<u64> {
    statm
        .split_whitespace()
        .nth(1)?
        .parse::<u64>()
        .ok()?
        .checked_mul(4)
}

/// `/proc/self/stat`'s user and system time in milliseconds, at 100 ticks a
/// second. The command name in brackets can hold spaces, so the fields are
/// counted from its closing bracket.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn stat_cpu_ms(stat: &str) -> Option<f64> {
    let rest = &stat[stat.rfind(')')? + 1..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // utime and stime are fields 14 and 15; `rest` starts at field 3.
    let ticks = fields.get(11)?.parse::<f64>().ok()? + fields.get(12)?.parse::<f64>().ok()?;
    Some(ticks * 10.0)
}

#[cfg(test)]
mod tests {
    use super::{Trace, ps_line, stat_cpu_ms, statm_kb};

    #[test]
    fn ps_lines_read_as_kb_and_milliseconds() {
        assert_eq!(ps_line("  45680   0:01.25\n"), Some((45680, 1250.0)));
        assert_eq!(ps_line("100 1:02:03.50"), Some((100, 3_723_500.0)));
        assert_eq!(ps_line("100 2-00:00:01.00"), Some((100, 172_801_000.0)));
        assert_eq!(ps_line(""), None);
        assert_eq!(ps_line("x 0:01.00"), None);
    }

    #[test]
    fn proc_files_read_as_kb_and_milliseconds() {
        assert_eq!(statm_kb("5000 1200 300 1 0 900 0"), Some(4800));
        let stat = "42 (tiles pane) S 1 42 42 0 -1 4194560 100 0 0 0 250 50 0 0 20 0 3 0";
        assert_eq!(stat_cpu_ms(stat), Some(3000.0));
        assert_eq!(stat_cpu_ms("garbage"), None);
    }

    #[test]
    fn a_trace_writes_time_event_and_fields() {
        let dir = std::env::temp_dir().join(format!("tiles-pane-trace-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("trace.txt");
        let trace = Trace::open(Some(&path), std::time::Instant::now()).unwrap();
        trace.line("write", format_args!("update=3 kind=hover bytes=10"));
        let text = std::fs::read_to_string(&path).unwrap();
        let words: Vec<&str> = text.split_whitespace().collect();
        assert!(words[0].parse::<f64>().is_ok());
        assert_eq!(words[1..], ["write", "update=3", "kind=hover", "bytes=10"]);
        assert!(!Trace::open(None, std::time::Instant::now()).unwrap().on());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
