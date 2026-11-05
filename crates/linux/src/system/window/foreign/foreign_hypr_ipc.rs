use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc;

use agent_desktop_core::Deadline;
use serde_json::Value;

const MAX_REPLY: usize = 16 * 1024 * 1024;

fn parse(bytes: &[u8]) -> Option<HashMap<u64, u32>> {
    let clients: Vec<Value> = serde_json::from_slice(bytes).ok()?;
    let mut result = HashMap::new();
    for client in clients {
        let Some((address, pid)) = entry(&client) else {
            continue;
        };
        if result.insert(address, pid).is_some() {
            return None;
        }
    }
    Some(result)
}

/// One client's address and pid; an entry missing either, or with a pid that
/// is not a positive process id, is skipped so its window stays unproven.
fn entry(client: &Value) -> Option<(u64, u32)> {
    let address = client.get("address")?.as_str()?.strip_prefix("0x")?;
    let address = u64::from_str_radix(address, 16).ok()?;
    let pid = u32::try_from(client.get("pid")?.as_u64()?).ok()?;
    (pid > 0).then_some((address, pid))
}

/// The command socket of one instance. A signature that is empty, names the
/// current or parent directory, or contains a separator cannot be a real
/// instance and never leaves the runtime directory.
fn socket_path(runtime: &OsStr, signature: &OsStr) -> Option<PathBuf> {
    let bytes = signature.as_bytes();
    if bytes.is_empty() || bytes == b"." || bytes == b".." || bytes.contains(&b'/') {
        return None;
    }
    let mut path = PathBuf::from(runtime);
    path.push("hypr");
    path.push(signature);
    path.push(".socket.sock");
    Some(path)
}

/// Connects within the deadline: the connect runs on its own thread so a
/// full backlog cannot hold the caller past it.
fn connect_by(path: PathBuf, deadline: Deadline) -> Option<UnixStream> {
    let remaining = deadline.remaining();
    if remaining.is_zero() {
        return None;
    }
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(UnixStream::connect(path));
    });
    receiver.recv_timeout(remaining).ok()?.ok()
}

pub(super) fn clients(deadline: Deadline) -> Option<HashMap<u64, u32>> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    let signature = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    let mut stream = connect_by(socket_path(&runtime, &signature)?, deadline)?;
    exchange(&mut stream, deadline)
}

fn exchange(stream: &mut UnixStream, deadline: Deadline) -> Option<HashMap<u64, u32>> {
    let remaining = deadline.remaining();
    if remaining.is_zero() {
        return None;
    }
    stream.set_write_timeout(Some(remaining)).ok()?;
    stream.write_all(b"j/clients").ok()?;
    read_reply(deadline, |buffer, remaining| {
        stream.set_read_timeout(Some(remaining))?;
        stream.read(buffer)
    })
}

fn read_reply(
    deadline: Deadline,
    mut read: impl FnMut(&mut [u8], std::time::Duration) -> std::io::Result<usize>,
) -> Option<HashMap<u64, u32>> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let remaining = deadline.remaining();
        if remaining.is_zero() {
            return None;
        }
        let count = read(&mut buffer, remaining).ok()?;
        if count == 0 {
            break;
        }
        if bytes.len() + count > MAX_REPLY {
            return None;
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    parse(&bytes)
}

#[cfg(test)]
#[path = "foreign_hypr_ipc_tests.rs"]
mod tests;
