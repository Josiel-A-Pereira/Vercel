use crate::system::display::display_mutter::LogicalMonitor;
use crate::system::display::display_wayland_record::OutputRecord;

fn rotated(transform: u32) -> bool {
    transform & 1 == 1
}

fn fallback_scale(record: &OutputRecord) -> f64 {
    if record.integer_scale > 0 {
        f64::from(record.integer_scale)
    } else {
        1.0
    }
}

/// The scale is the ratio of the mode's pixels to the logical size along the
/// same axis, so a fractional scale survives even though `wl_output` only
/// carries an integer. A quarter turn swaps which mode side faces the logical
/// width.
fn scale_of(record: &OutputRecord, logical_width: i32) -> f64 {
    let Some((mode_width, mode_height)) = record.mode else {
        return fallback_scale(record);
    };
    let pixels = if rotated(record.transform) {
        mode_height
    } else {
        mode_width
    };
    if pixels > 0 && logical_width > 0 {
        f64::from(pixels) / f64::from(logical_width)
    } else {
        fallback_scale(record)
    }
}

fn name_of(record: &OutputRecord) -> String {
    record
        .logical_name
        .clone()
        .or_else(|| record.output_name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| format!("wl-output-{}", record.global))
}

fn monitor(record: &OutputRecord) -> Option<LogicalMonitor> {
    let (x, y) = record.position?;
    let (width, height) = record.size?;
    if width <= 0 || height <= 0 {
        return None;
    }
    Some(LogicalMonitor {
        x,
        y,
        width,
        height,
        scale: scale_of(record, width),
        primary: false,
        connectors: vec![name_of(record)],
    })
}

/// Wayland has no primary output. The one at the layout origin stands in for
/// it, and a layout without an output there falls back to the left-most and
/// then top-most one.
fn mark_primary(monitors: &mut [LogicalMonitor]) {
    let chosen = monitors
        .iter()
        .position(|monitor| monitor.x == 0 && monitor.y == 0)
        .or_else(|| {
            monitors
                .iter()
                .enumerate()
                .min_by_key(|(_, monitor)| (monitor.x, monitor.y))
                .map(|(index, _)| index)
        });
    if let Some(index) = chosen {
        monitors[index].primary = true;
    }
}

/// Turns the protocol records into logical monitors. An output that never
/// reported a logical position and size, or reports an empty one because it is
/// switched off, is left out.
pub(crate) fn assemble(records: &[OutputRecord]) -> Vec<LogicalMonitor> {
    let mut monitors: Vec<LogicalMonitor> = records.iter().filter_map(monitor).collect();
    mark_primary(&mut monitors);
    monitors
}

#[cfg(test)]
#[path = "display_wayland_assemble_tests.rs"]
mod tests;
