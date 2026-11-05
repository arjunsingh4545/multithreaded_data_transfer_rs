//! Human-readable plan, summary and probe output.

use std::fmt::Write as _;
use std::path::Path;
use std::time::Duration;

use super::output::Output;
use crate::domain::device::{DiskInfo, DiskKind};
use crate::engine::{Prepared, Summary};
use crate::util::units::ByteSize;

/// `1234567` → `1,234,567`.
pub fn group_digits(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// `3.2s`, `4m 05s`, `1h 02m 03s`.
pub fn human_duration(d: Duration) -> String {
    let secs = d.as_secs();
    match secs {
        0..60 => format!("{:.1}s", d.as_secs_f64()),
        60..3600 => format!("{}m {:02}s", secs / 60, secs % 60),
        _ => format!(
            "{}h {:02}m {:02}s",
            secs / 3600,
            (secs % 3600) / 60,
            secs % 60
        ),
    }
}

/// `sda · HDD · max request 4 MiB`.
pub fn describe_disk(disk: &DiskInfo) -> String {
    let mut text = format!("{} · {}", disk.name, disk.kind);
    if let Some(max) = disk.max_request {
        // Writing to a String cannot fail.
        let _ = write!(text, " · max request {max}");
    }
    text
}

/// The pre-copy plan block.
pub fn plan(out: &Output, prepared: &Prepared) {
    let topology = prepared.topology();
    let plan = prepared.plan();
    out.field(
        "source",
        format!(
            "{}  ({})",
            prepared.src().display(),
            describe_disk(&topology.src)
        ),
    );
    out.field(
        "destination",
        format!(
            "{}  ({})",
            prepared.dst().display(),
            describe_disk(&topology.dst)
        ),
    );
    out.field("plan", format!("{} · {}", plan.transfer, plan.strategy));
    out.detail(format!("why: {}", plan.reason));

    if topology.src.kind == DiskKind::Unknown || topology.dst.kind == DiskKind::Unknown {
        out.hint("disk type not detected on one side (treated as SSD); use --src-disk/--dst-disk hdd if it is a hard disk");
    }

    let manifest = prepared.manifest();
    let mut line = format!(
        "{} files, {} dirs, {} symlinks · {} to copy",
        group_digits(manifest.files.len() as u64),
        group_digits(manifest.directories.len() as u64),
        group_digits(manifest.symlinks.len() as u64),
        ByteSize::new(manifest.total_bytes),
    );
    let skipped = prepared.filtered();
    if skipped.skipped > 0 {
        let _ = write!(
            line,
            " (skipping {} existing, {})",
            group_digits(skipped.skipped as u64),
            ByteSize::new(skipped.skipped_bytes)
        );
    }
    out.success(line);
}

/// The post-copy summary.
pub fn summary(out: &Output, summary: &Summary) {
    if summary.stats.files == 0 {
        out.success("nothing to copy: destination is up to date");
        return;
    }
    out.success(format!(
        "copied {} files ({}) in {} · {}/s",
        group_digits(summary.stats.files as u64),
        ByteSize::new(summary.stats.bytes),
        human_duration(summary.elapsed),
        ByteSize::new(summary.throughput()),
    ));
    if summary.stats.metadata_failures > 0 {
        out.warn(format!(
            "{} files kept default permissions/times (the destination filesystem refused them)",
            group_digits(summary.stats.metadata_failures as u64)
        ));
    }
}

/// One `probe` row.
pub fn probe_row(out: &Output, path: &Path, disk: &DiskInfo) {
    out.field(
        &path.display().to_string(),
        format!("{}  [{}]", describe_disk(disk), disk.id),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::device::DeviceId;

    #[test]
    fn groups_thousands() {
        assert_eq!(group_digits(1_234_567), "1,234,567");
    }

    #[test]
    fn small_numbers_have_no_separator() {
        assert_eq!(group_digits(999), "999");
    }

    #[test]
    fn formats_minutes_with_padded_seconds() {
        assert_eq!(human_duration(Duration::from_secs(245)), "4m 05s");
    }

    #[test]
    fn formats_hours() {
        assert_eq!(human_duration(Duration::from_secs(3723)), "1h 02m 03s");
    }

    #[test]
    fn describes_disk_with_request_size() {
        let disk = DiskInfo {
            max_request: Some(ByteSize::mib(4)),
            ..DiskInfo::new(DeviceId::new(8, 0), "sda", DiskKind::Hdd)
        };
        assert_eq!(describe_disk(&disk), "sda · HDD · max request 4 MiB");
    }
}
