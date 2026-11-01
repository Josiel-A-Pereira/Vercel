use std::path::Path;

use crate::system::process::process_facts;

const INTERPRETERS: [&str; 8] = ["python", "perl", "ruby", "node", "gjs", "lua", "sh", "bash"];
const MIN_CONTAINED: usize = 4;

/// Every name a process answers to, used to tell whether a toplevel's app id
/// belongs to it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::system) struct ProcessNames {
    names: Vec<String>,
}

fn normalised(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|character| character.to_ascii_lowercase())
        .collect()
}

fn basename(text: &str) -> Option<String> {
    Path::new(text)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

fn argv_of(pid: u32) -> Vec<String> {
    let Ok(raw) = std::fs::read(format!("/proc/{pid}/cmdline")) else {
        return Vec::new();
    };
    raw.split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect()
}

fn is_interpreter(executable: &str) -> bool {
    let lower = executable.to_ascii_lowercase();
    INTERPRETERS.iter().any(|name| lower.starts_with(name))
}

fn related(left: &str, right: &str) -> bool {
    let (shorter, longer) = if left.len() <= right.len() {
        (left, right)
    } else {
        (right, left)
    };
    shorter == longer || (shorter.len() >= MIN_CONTAINED && longer.contains(shorter))
}

impl ProcessNames {
    #[cfg(test)]
    pub(in crate::system) fn from_names(names: Vec<String>) -> Self {
        Self { names }
    }

    /// The names of one process: the accessibility app name, its comm and
    /// executable names, `argv[0]`, and for a script run by an interpreter
    /// the script's own name from `argv[1]`.
    pub(in crate::system) fn of(pid: u32, app_name: &str) -> Self {
        let mut names = vec![app_name.to_owned()];
        let facts = process_facts::read(pid);
        let executable = facts.as_ref().and_then(|facts| facts.exe_basename.clone());
        if let Some(facts) = facts {
            names.push(facts.comm);
        }
        let argv = argv_of(pid);
        let program = argv.first().and_then(|first| basename(first));
        let interpreted = executable
            .as_deref()
            .or(program.as_deref())
            .is_some_and(is_interpreter);
        names.extend(executable);
        names.extend(program);
        if interpreted && let Some(script) = argv.get(1).filter(|script| !script.starts_with('-')) {
            names.extend(basename(script));
        }
        names.retain(|name| !normalised(name).is_empty());
        let mut seen: Vec<String> = Vec::new();
        names.retain(|name| {
            let new = !seen.contains(name);
            if new {
                seen.push(name.clone());
            }
            new
        });
        Self { names }
    }

    pub(in crate::system) fn relates(&self, app_id: &str) -> bool {
        let wanted = normalised(app_id);
        !wanted.is_empty()
            && self
                .names
                .iter()
                .map(|name| normalised(name))
                .any(|name| !name.is_empty() && related(&name, &wanted))
    }
}

#[cfg(test)]
#[path = "foreign_names_tests.rs"]
mod tests;
