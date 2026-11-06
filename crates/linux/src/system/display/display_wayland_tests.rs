use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use agent_desktop_core::{Deadline, ErrorCode};

use super::*;

const DISPLAY: u32 = 1;
const REGISTRY: u32 = 2;
const SYNC_OPCODE: u32 = 0;
const REGISTRY_BIND: u32 = 0;
const MANAGER_GET_XDG_OUTPUT: u32 = 1;

struct Request {
    object: u32,
    opcode: u32,
    body: Vec<u8>,
}

struct Binding {
    name: u32,
    id: u32,
}

fn deadline(millis: u64) -> Deadline {
    Deadline::after(millis).expect("a deadline builds")
}

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("four bytes"))
}

fn read_request(stream: &mut UnixStream) -> Request {
    let mut header = [0u8; 8];
    stream.read_exact(&mut header).expect("a request header");
    let size_and_opcode = word(&header, 4);
    let size = (size_and_opcode >> 16) as usize;
    let mut body = vec![0u8; size - 8];
    stream.read_exact(&mut body).expect("a request body");
    Request {
        object: word(&header, 0),
        opcode: size_and_opcode & 0xffff,
        body,
    }
}

fn is_sync(request: &Request) -> bool {
    request.object == DISPLAY && request.opcode == SYNC_OPCODE
}

fn read_until_sync(stream: &mut UnixStream) -> Vec<Request> {
    let mut requests = Vec::new();
    loop {
        let request = read_request(stream);
        let done = is_sync(&request);
        requests.push(request);
        if done {
            return requests;
        }
    }
}

fn sync_callback(requests: &[Request]) -> u32 {
    let sync = requests.last().expect("a sync request");
    word(&sync.body, 0)
}

fn string_arg(text: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    let length = u32::try_from(text.len() + 1).expect("a short string");
    bytes.extend_from_slice(&length.to_le_bytes());
    bytes.extend_from_slice(text.as_bytes());
    bytes.push(0);
    while bytes.len() % 4 != 0 {
        bytes.push(0);
    }
    bytes
}

fn words(values: &[i64]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| (*value as i32).to_le_bytes())
        .collect()
}

fn event(object: u32, opcode: u32, args: &[u8]) -> Vec<u8> {
    let size = u32::try_from(8 + args.len()).expect("a short event");
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&object.to_le_bytes());
    bytes.extend_from_slice(&((size << 16) | opcode).to_le_bytes());
    bytes.extend_from_slice(args);
    bytes
}

fn global(name: u32, interface: &str, version: u32) -> Vec<u8> {
    let mut args = name.to_le_bytes().to_vec();
    args.extend(string_arg(interface));
    args.extend(version.to_le_bytes());
    event(REGISTRY, 0, &args)
}

fn callback_done(callback: u32) -> Vec<u8> {
    event(callback, 0, &words(&[0]))
}

fn bindings(requests: &[Request]) -> Vec<Binding> {
    requests
        .iter()
        .filter(|request| request.object == REGISTRY && request.opcode == REGISTRY_BIND)
        .map(|request| {
            let interface_length = word(&request.body, 4) as usize;
            let padded = interface_length.div_ceil(4) * 4;
            Binding {
                name: word(&request.body, 0),
                id: word(&request.body, 8 + padded + 4),
            }
        })
        .collect()
}

fn xdg_output_id(requests: &[Request], manager: u32) -> u32 {
    let request = requests
        .iter()
        .find(|request| request.object == manager && request.opcode == MANAGER_GET_XDG_OUTPUT)
        .expect("a get_xdg_output request");
    word(&request.body, 0)
}

fn spawn_compositor(
    mut stream: UnixStream,
    script: impl FnOnce(&mut UnixStream) + Send + 'static,
) -> JoinHandle<UnixStream> {
    thread::spawn(move || {
        script(&mut stream);
        stream
    })
}

fn send(stream: &mut UnixStream, bytes: &[u8]) {
    stream.write_all(bytes).expect("the client is listening");
}

fn first_round(stream: &mut UnixStream, globals: &[(u32, &str, u32)]) {
    let requests = read_until_sync(stream);
    let mut reply = Vec::new();
    for (name, interface, version) in globals {
        reply.extend(global(*name, interface, *version));
    }
    reply.extend(callback_done(sync_callback(&requests)));
    send(stream, &reply);
}

fn output_events(output: u32) -> Vec<u8> {
    let mut geometry = words(&[0, 0, 600, 340, 0]);
    geometry.extend(string_arg("Acme"));
    geometry.extend(string_arg("Panel"));
    geometry.extend(words(&[0]));
    let mut events = event(output, 0, &geometry);
    events.extend(event(output, 1, &words(&[0x1, 3840, 2160, 60000])));
    events.extend(event(output, 3, &words(&[2])));
    events.extend(event(output, 4, &string_arg("DP-9")));
    events.extend(event(output, 2, &[]));
    events
}

fn logical_events(xdg_output: u32) -> Vec<u8> {
    let mut events = event(xdg_output, 0, &words(&[100, 50]));
    events.extend(event(xdg_output, 1, &words(&[1920, 1080])));
    events.extend(event(xdg_output, 3, &string_arg("DP-9")));
    events
}

#[test]
fn a_silent_compositor_times_out_at_the_deadline() {
    let (client, _compositor) = UnixStream::pair().expect("a socket pair");
    let started = Instant::now();
    let error = read_from(client, deadline(200)).expect_err("a silent compositor times out");
    assert_eq!(error.code, ErrorCode::Timeout);
    assert!(started.elapsed() >= Duration::from_millis(150));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn a_compositor_that_hangs_up_means_no_monitors() {
    let (client, compositor) = UnixStream::pair().expect("a socket pair");
    drop(compositor);
    let monitors = read_from(client, deadline(2_000)).expect("a broken connection is not an error");
    assert_eq!(monitors, None);
}

#[test]
fn a_compositor_without_xdg_output_means_no_monitors() {
    let (client, compositor) = UnixStream::pair().expect("a socket pair");
    let handle = spawn_compositor(compositor, |stream| {
        first_round(stream, &[(1, "wl_output", 4)]);
    });
    let monitors = read_from(client, deadline(2_000)).expect("a missing manager is not an error");
    assert_eq!(monitors, None);
    handle.join().expect("the compositor thread ends");
}

#[test]
fn a_compositor_without_any_output_means_no_monitors() {
    let (client, compositor) = UnixStream::pair().expect("a socket pair");
    let handle = spawn_compositor(compositor, |stream| {
        first_round(stream, &[(2, "zxdg_output_manager_v1", 3)]);
    });
    let monitors = read_from(client, deadline(2_000)).expect("no output is not an error");
    assert_eq!(monitors, None);
    handle.join().expect("the compositor thread ends");
}

#[test]
fn a_compositor_that_stalls_in_the_second_round_times_out() {
    let (client, compositor) = UnixStream::pair().expect("a socket pair");
    let handle = spawn_compositor(compositor, |stream| {
        first_round(
            stream,
            &[(1, "wl_output", 4), (2, "zxdg_output_manager_v1", 3)],
        );
        read_until_sync(stream);
    });
    let error = read_from(client, deadline(300)).expect_err("a stalled round times out");
    assert_eq!(error.code, ErrorCode::Timeout);
    handle.join().expect("the compositor thread ends");
}

#[test]
fn a_full_exchange_yields_the_logical_monitor() {
    let (client, compositor) = UnixStream::pair().expect("a socket pair");
    let handle = spawn_compositor(compositor, |stream| {
        first_round(
            stream,
            &[(1, "wl_output", 4), (2, "zxdg_output_manager_v1", 3)],
        );
        let requests = read_until_sync(stream);
        let binds = bindings(&requests);
        let output = binds
            .iter()
            .find(|bind| bind.name == 1)
            .expect("output bind");
        let manager = binds
            .iter()
            .find(|bind| bind.name == 2)
            .expect("manager bind");
        let mut reply = output_events(output.id);
        reply.extend(logical_events(xdg_output_id(&requests, manager.id)));
        reply.extend(callback_done(sync_callback(&requests)));
        send(stream, &reply);
    });
    let monitors = read_from(client, deadline(5_000))
        .expect("the exchange completes")
        .expect("one monitor is reported");
    handle.join().expect("the compositor thread ends");
    let [monitor] = monitors.as_slice() else {
        panic!("one output was advertised: {monitors:?}")
    };
    assert_eq!((monitor.x, monitor.y), (100, 50));
    assert_eq!((monitor.width, monitor.height), (1920, 1080));
    assert!((monitor.scale - 2.0).abs() < 1e-9);
    assert!(monitor.primary);
    assert_eq!(monitor.connectors, vec!["DP-9".to_string()]);
}

#[test]
fn a_slow_compositor_within_the_deadline_still_answers() {
    let (client, compositor) = UnixStream::pair().expect("a socket pair");
    let handle = spawn_compositor(compositor, |stream| {
        let requests = read_until_sync(stream);
        thread::sleep(Duration::from_millis(300));
        send(stream, &callback_done(sync_callback(&requests)));
    });
    let monitors = read_from(client, deadline(5_000)).expect("a slow answer still counts");
    assert_eq!(monitors, None);
    handle.join().expect("the compositor thread ends");
}

#[test]
#[ignore = "needs a live Wayland session; run with WAYLAND_DISPLAY set"]
fn wayland_live_reads_at_least_one_monitor() {
    let monitors = logical_monitors(deadline(10_000)).expect("the live read does not time out");
    println!("wayland_live monitors={monitors:?}");
    let Some(monitors) = monitors else {
        panic!("the session offered no wl_output and xdg_output pair")
    };
    assert!(!monitors.is_empty());
    assert!(
        monitors
            .iter()
            .all(|monitor| monitor.width > 0 && monitor.height > 0)
    );
}
