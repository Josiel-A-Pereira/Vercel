use super::assemble;
use crate::system::display::display_wayland_record::OutputRecord;

fn record(global: u32, position: (i32, i32), size: (i32, i32)) -> OutputRecord {
    OutputRecord {
        global,
        position: Some(position),
        size: Some(size),
        mode: Some(size),
        integer_scale: 1,
        ..OutputRecord::default()
    }
}

#[test]
fn one_output_at_the_origin_is_the_primary_at_scale_one() {
    let mut output = record(7, (0, 0), (1920, 1080));
    output.logical_name = Some("DP-1".to_owned());
    let monitors = assemble(&[output]);
    let [monitor] = monitors.as_slice() else {
        panic!("one monitor");
    };
    assert_eq!(monitor.connectors, vec!["DP-1".to_owned()]);
    assert_eq!((monitor.x, monitor.y), (0, 0));
    assert_eq!((monitor.width, monitor.height), (1920, 1080));
    assert!(monitor.primary);
    assert!((monitor.scale - 1.0).abs() < f64::EPSILON);
}

#[test]
fn a_fractional_scale_comes_from_the_mode_and_the_logical_size() {
    let mut output = record(1, (0, 0), (2560, 1440));
    output.mode = Some((3840, 2160));
    output.integer_scale = 2;
    let monitors = assemble(&[output]);
    assert!((monitors[0].scale - 1.5).abs() < 1e-9);
}

#[test]
fn a_quarter_turn_pairs_the_mode_height_with_the_logical_width() {
    let mut output = record(1, (0, 0), (1440, 2560));
    output.mode = Some((3840, 2160));
    output.transform = 1;
    let monitors = assemble(&[output]);
    assert!((monitors[0].scale - 1.5).abs() < 1e-9);
}

#[test]
fn a_flipped_quarter_turn_counts_as_rotated() {
    let mut output = record(1, (0, 0), (1440, 2560));
    output.mode = Some((3840, 2160));
    output.transform = 5;
    let monitors = assemble(&[output]);
    assert!((monitors[0].scale - 1.5).abs() < 1e-9);
}

#[test]
fn a_half_turn_keeps_the_mode_width() {
    let mut output = record(1, (0, 0), (2560, 1440));
    output.mode = Some((3840, 2160));
    output.transform = 2;
    let monitors = assemble(&[output]);
    assert!((monitors[0].scale - 1.5).abs() < 1e-9);
}

#[test]
fn a_missing_mode_falls_back_to_the_integer_scale() {
    let mut output = record(1, (0, 0), (960, 540));
    output.mode = None;
    output.integer_scale = 2;
    let monitors = assemble(&[output]);
    assert!((monitors[0].scale - 2.0).abs() < f64::EPSILON);
}

#[test]
fn no_scale_at_all_means_one() {
    let mut output = record(1, (0, 0), (960, 540));
    output.mode = None;
    output.integer_scale = 0;
    let monitors = assemble(&[output]);
    assert!((monitors[0].scale - 1.0).abs() < f64::EPSILON);
}

#[test]
fn the_output_at_the_origin_is_the_primary_whatever_its_position_in_the_list() {
    let right = record(2, (1920, 0), (2560, 1440));
    let left = record(1, (0, 180), (1920, 1080));
    let origin = record(3, (0, 0), (1280, 720));
    let monitors = assemble(&[right, left, origin]);
    let primaries: Vec<u32> = monitors
        .iter()
        .enumerate()
        .filter(|(_, monitor)| monitor.primary)
        .map(|(index, _)| index as u32)
        .collect();
    assert_eq!(primaries, vec![2]);
}

#[test]
fn a_layout_without_an_origin_output_picks_the_left_most_as_primary() {
    let right = record(2, (1920, 0), (2560, 1440));
    let left = record(1, (0, 180), (1920, 1080));
    let monitors = assemble(&[right, left]);
    assert!(!monitors[0].primary);
    assert!(monitors[1].primary);
}

#[test]
fn the_left_most_output_wins_before_the_top_most() {
    let high_right = record(2, (1920, 0), (1920, 1080));
    let low_left = record(1, (-1920, 400), (1920, 1080));
    let monitors = assemble(&[high_right, low_left]);
    assert!(!monitors[0].primary);
    assert!(monitors[1].primary);
}

#[test]
fn the_logical_name_beats_the_output_name_and_the_global_id_is_the_last_resort() {
    let mut both = record(1, (0, 0), (1920, 1080));
    both.logical_name = Some("DP-1".to_owned());
    both.output_name = Some("other".to_owned());
    let mut output_only = record(2, (1920, 0), (1920, 1080));
    output_only.output_name = Some("HDMI-A-1".to_owned());
    let anonymous = record(9, (3840, 0), (1920, 1080));
    let mut empty = record(4, (5760, 0), (1920, 1080));
    empty.logical_name = Some(String::new());
    let names: Vec<String> = assemble(&[both, output_only, anonymous, empty])
        .into_iter()
        .map(|monitor| monitor.connectors.join("+"))
        .collect();
    assert_eq!(
        names,
        vec!["DP-1", "HDMI-A-1", "wl-output-9", "wl-output-4"]
    );
}

#[test]
fn an_output_without_a_logical_position_or_size_is_left_out() {
    let mut no_position = record(1, (0, 0), (1920, 1080));
    no_position.position = None;
    let mut no_size = record(2, (0, 0), (1920, 1080));
    no_size.size = None;
    let kept = record(3, (0, 0), (1280, 720));
    let monitors = assemble(&[no_position, no_size, kept]);
    assert_eq!(monitors.len(), 1);
    assert_eq!(monitors[0].width, 1280);
}

#[test]
fn a_switched_off_output_reporting_an_empty_size_is_left_out() {
    let off = record(1, (0, 0), (0, 0));
    let on = record(2, (0, 0), (1920, 1080));
    let monitors = assemble(&[off, on]);
    assert_eq!(monitors.len(), 1);
    assert!(monitors[0].primary);
}

#[test]
fn no_outputs_gives_no_monitors() {
    assert!(assemble(&[]).is_empty());
}
