//! `mcp --connect` to a port where no app listens tells the user how to start one (#270).

use std::net::TcpListener;
use std::process::{Command, Stdio};

#[test]
fn mcp_connect_without_a_listening_app_names_the_control_flag() {
    // A port that was free a moment ago: nothing listens on it.
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let output = Command::new(env!("CARGO_BIN_EXE_solidcraft-cli"))
        .args(["mcp", "--connect", &format!("127.0.0.1:{port}")])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(stderr.contains("--control"), "say how to start the app with a control port: {stderr}");
}

/// Quiet mode hides startup paths while keeping stdout and file confinement intact.
#[test]
fn quiet_mcp_keeps_protocol_and_rejects_files_outside_explicit_root() {
    use std::io::Write;
    let root = std::env::temp_dir().join(format!("solidcraft-quiet-mcp-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let outside = root.parent().unwrap().join("outside-quiet-proof.solidcraft");
    let mut child = Command::new(env!("CARGO_BIN_EXE_solidcraft-cli"))
        .arg("mcp")
        .arg("--quiet")
        .arg("--allow-path")
        .arg(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let request = serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"open","arguments":{"path":outside}}});
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{}", serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"quiet-proof","version":"0"}}})).unwrap();
    writeln!(stdin, "{request}").unwrap();
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{:?}", output.status);
    assert!(output.stderr.is_empty(), "{}", String::from_utf8_lossy(&output.stderr));
    let lines: Vec<serde_json::Value> = String::from_utf8(output.stdout).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0]["result"]["serverInfo"].is_object());
    assert_eq!(lines[1]["result"]["isError"], true);
    let error = lines[1].to_string();
    assert!(error.contains("allow-path") || error.contains("outside") || error.contains("allowed"), "{error}");
    std::fs::remove_dir(&root).unwrap();
}
