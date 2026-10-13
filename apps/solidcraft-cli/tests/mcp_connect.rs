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
