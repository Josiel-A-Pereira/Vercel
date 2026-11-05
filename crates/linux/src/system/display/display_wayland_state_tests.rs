use super::*;

fn state_with_output() -> WaylandState {
    let mut state = WaylandState::default();
    state.records.push(OutputRecord {
        global: 7,
        integer_scale: 1,
        ..Default::default()
    });
    state
}

#[test]
fn only_the_current_mode_is_stored() {
    let mut state = state_with_output();
    state.on_output_mode(0, false, 800, 600);
    assert_eq!(state.records[0].mode, None);
    state.on_output_mode(0, true, 1920, 1080);
    assert_eq!(state.records[0].mode, Some((1920, 1080)));
}

#[test]
fn output_and_logical_facts_land_on_their_record() {
    let mut state = state_with_output();
    state.on_output_transform(0, 3);
    state.on_output_scale(0, 2);
    state.on_output_name(0, "DP-1".into());
    state.on_logical_position(0, 10, 20);
    state.on_logical_size(0, 960, 540);
    state.on_logical_name(0, "DP-1".into());
    let record = &state.records[0];
    assert_eq!(record.transform, 3);
    assert_eq!(record.integer_scale, 2);
    assert_eq!(record.output_name.as_deref(), Some("DP-1"));
    assert_eq!(record.position, Some((10, 20)));
    assert_eq!(record.size, Some((960, 540)));
    assert_eq!(record.logical_name.as_deref(), Some("DP-1"));
}

#[test]
fn an_unknown_index_is_ignored() {
    let mut state = state_with_output();
    state.on_output_scale(5, 3);
    state.on_logical_size(5, 1, 1);
    assert_eq!(state.records[0].integer_scale, 1);
}

#[test]
fn globals_are_recorded_in_order() {
    let mut state = WaylandState::default();
    state.on_global(1, "wl_output".into(), 4);
    state.on_global(2, "zxdg_output_manager_v1".into(), 3);
    assert_eq!(state.globals.len(), 2);
    assert_eq!(state.globals[1].interface, "zxdg_output_manager_v1");
}
