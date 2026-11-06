use super::*;

#[test]
fn mapping_replies_join_ipc_pids_by_exact_address() {
    let mappings = HashMap::from([(1, 0x1234), (2, 0x5678), (3, 0x9999)]);
    let clients = HashMap::from([(0x1234, 7), (0x5678, 8)]);
    assert_eq!(join(&mappings, &clients), HashMap::from([(1, 7), (2, 8)]));
}

#[test]
fn duplicate_mapping_addresses_are_not_accepted_as_ownership() {
    let mappings = HashMap::from([(1, 0x1234), (2, 0x1234), (3, 0x5678)]);
    let clients = HashMap::from([(0x1234, 7), (0x5678, 8)]);
    assert_eq!(join(&mappings, &clients), HashMap::from([(3, 8)]));
}

#[test]
fn an_address_that_changes_process_between_ipc_reads_is_unproven() {
    let before = HashMap::from([(0x1234, 7), (0x5678, 8)]);
    let after = HashMap::from([(0x1234, 9), (0x5678, 8)]);
    assert_eq!(
        stable_clients(&before, &after),
        HashMap::from([(0x5678, 8)])
    );
}
