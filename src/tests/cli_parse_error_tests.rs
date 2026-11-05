use super::binary;

#[test]
fn clap_parse_failure_names_typed_subcommand() {
    let output = binary()
        .args(["list-windows", "--window-id", "x"])
        .output()
        .expect("binary starts");
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout is one JSON envelope");

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(envelope["command"], "list-windows");
}

#[test]
fn clap_parse_failure_finds_subcommand_after_global_option_and_value() {
    let output = binary()
        .args(["--timeout-ms", "37", "list-windows", "--window-id", "x"])
        .output()
        .expect("binary starts");
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout is one JSON envelope");

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(envelope["command"], "list-windows");
}

#[cfg(target_os = "linux")]
#[test]
fn unwritable_stdout_and_stderr_do_not_abort_the_cli() {
    let full = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .expect("Linux provides /dev/full");
    let output = super::binary()
        .arg("--help")
        .stdout(std::process::Stdio::from(
            full.try_clone().expect("clone /dev/full"),
        ))
        .stderr(std::process::Stdio::from(full))
        .status()
        .expect("binary starts");
    assert_eq!(output.code(), Some(1), "broken output must not abort");
}
