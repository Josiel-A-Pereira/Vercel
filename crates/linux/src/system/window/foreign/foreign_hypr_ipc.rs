use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use agent_desktop_core::Deadline;
use serde_json::Value;

const MAX_REPLY: usize = 16 * 1024 * 1024;

fn parse(bytes: &[u8]) -> Option<HashMap<u64, u32>> {
    let clients: Vec<Value> = serde_json::from_slice(bytes).ok()?;
    let mut result = HashMap::new();
    for client in clients {
        let address = client.get("address")?.as_str()?.strip_prefix("0x")?;
        let address = u64::from_str_radix(address, 16).ok()?;
        let pid = u32::try_from(client.get("pid")?.as_u64()?).ok()?;
        if result.insert(address, pid).is_some() {
            return None;
        }
    }
    Some(result)
}

pub(super) fn clients(deadline: Deadline) -> Option<HashMap<u64, u32>> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    let signature = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    let mut path = PathBuf::from(runtime);
    path.push("hypr");
    path.push(signature);
    path.push(".socket.sock");
    let mut stream = UnixStream::connect(path).ok()?;
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
