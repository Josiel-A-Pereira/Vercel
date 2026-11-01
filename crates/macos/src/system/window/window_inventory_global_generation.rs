//! Binds each window record's process instance to the NSWorkspace owner it
//! is reported under.
//!
//! NSWorkspace's launch date is the primary evidence, but it is not always the
//! launch date of the live process: a long-running TextEdit was observed with
//! a launch date 12 days older than its libproc start time. Treating that as
//! churn failed every global window inventory for every app. When the launch
//! date disagrees, the owner is still accepted if the captured instance is the
//! live process and WindowServer names every window's owner exactly as
//! NSWorkspace names the app. A pid reused by a different app fails the name
//! check, and a restart during capture still fails the snapshot-pair check.

use super::{OwnerGeneration, OwnerSnapshotView, owner_churn_error};
use crate::system::process::process_identity;
use crate::system::window::cg_window::WindowRecord;
use agent_desktop_core::AdapterError;
use rustc_hash::FxHashMap;

pub(super) fn validate_record_generations<S: OwnerSnapshotView>(
    records: &[WindowRecord],
    owners: &S,
) -> Result<(), AdapterError> {
    for (pid, instance) in unique_process_instances(records)? {
        let generation = owners
            .generation(pid)
            .ok_or_else(|| owner_churn_error(Some(pid), "owner_not_eligible"))?;
        let matches = match generation {
            OwnerGeneration::LaunchTime(launch_time) => {
                process_identity::instance_matches_launch_time(pid, instance, launch_time)?
                    || same_app_outlived_its_launch_date(pid, instance, records, owners)?
            }
            OwnerGeneration::LiveProcess => process_identity::matches_instance(pid, instance)?,
        };
        if !matches {
            return Err(owner_churn_error(Some(pid), "process_generation_changed"));
        }
    }
    Ok(())
}

fn same_app_outlived_its_launch_date<S: OwnerSnapshotView>(
    pid: i32,
    instance: &str,
    records: &[WindowRecord],
    owners: &S,
) -> Result<bool, AdapterError> {
    let Some(owner_name) = owners.owner_name(pid) else {
        return Ok(false);
    };
    let window_owner_names_match = records
        .iter()
        .filter(|record| record.pid == pid)
        .all(|record| record.app_name == owner_name);
    if !window_owner_names_match {
        return Ok(false);
    }
    process_identity::matches_instance(pid, instance)
}

pub(super) fn unique_process_instances(
    records: &[WindowRecord],
) -> Result<Vec<(i32, &str)>, AdapterError> {
    let mut instances = FxHashMap::default();
    for record in records {
        let instance = record
            .process_instance
            .as_deref()
            .filter(|instance| !instance.is_empty())
            .ok_or_else(|| owner_churn_error(Some(record.pid), "process_instance_missing"))?;
        match instances.entry(record.pid) {
            std::collections::hash_map::Entry::Occupied(entry) => {
                if *entry.get() != instance {
                    return Err(owner_churn_error(
                        Some(record.pid),
                        "process_instance_conflict",
                    ));
                }
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(instance);
            }
        }
    }
    let mut instances = instances.into_iter().collect::<Vec<_>>();
    instances.sort_unstable_by_key(|(pid, _)| *pid);
    Ok(instances)
}

#[cfg(test)]
#[path = "window_inventory_global_generation_tests.rs"]
mod tests;
