use crate::common::{binary, TempDir};

fn profile(mode: &str) -> String {
    format!("[network]\nmode='{mode}'\n[[network.allow]]\ndestination={{dns='ＥＸＡＭＰＬＥ.com'}}\nprotocol='tcp'\nports=['443']\n[[network.publish]]\nmode='fixed'\nprotocol='tcp'\nport=8000\nhost-port=18000\n")
}

// @kotowari[REQ-013, REQ-390, REQ-394, EX-165, REQ-431, EX-843]
#[test]
fn filtered_plan_shows_normalized_intent_without_starting_network_tools() {
    let home = TempDir::new();
    let work = home.path().join("project");
    std::fs::create_dir(&work).unwrap();
    home.write(".config/kakoi/profile/default.toml", profile("filtered"));
    let tools = home.path().join("tools");
    for name in ["pasta", "nft"] {
        home.write_executable(
            format!("tools/{name}"),
            "#!/bin/sh\nprintf called > \"$0.called\"\nexit 99\n",
        );
    }
    let mut paths = vec![tools.clone()];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let output = binary(home.path())
        .env("PATH", std::env::join_paths(paths).unwrap())
        .args(["--print-plan=json", "--workspace"])
        .arg(&work)
        .current_dir(&work)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let policy = &json["policy"];
    assert_eq!(policy["network_mode"], "filtered");
    assert!(output
        .stdout
        .windows(b"example.com".len())
        .any(|part| part == b"example.com"));
    assert_eq!(policy["network_publish"][0]["host_port"], 18000);
    assert_eq!(policy["network_limits"]["dns_server_timeout_seconds"], 2);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("published"));
    assert!(!tools.join("pasta.called").exists());
    assert!(!tools.join("nft.called").exists());
}

// @kotowari[REQ-096, REQ-395]
#[test]
fn inactive_network_settings_warn_without_starting_network_tools() {
    let home = TempDir::new();
    let work = home.path().join("project");
    std::fs::create_dir(&work).unwrap();
    home.write(
        ".config/kakoi/profile/default.toml",
        format!("{}\n[process]\nshutdown-grace-seconds=10", profile("host")),
    );
    let output = binary(home.path())
        .args(["--print-plan", "--workspace"])
        .arg(&work)
        .current_dir(&work)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("unused"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("allow tcp example.com ports 443"));
}

// @kotowari[REQ-013, REQ-394, EX-183]
#[test]
fn full_plan_exposes_network_rules_and_effective_time_limits() {
    let home = TempDir::new();
    let work = home.path().join("project");
    std::fs::create_dir(&work).unwrap();
    home.write(
        ".config/kakoi/profile/default.toml",
        format!(
            "{}\n[process]\nshutdown-grace-seconds=9",
            profile("filtered").replace(
                "mode='filtered'",
                "mode='filtered'\ndns-zero-ttl-grace-milliseconds=750"
            )
        ),
    );
    let output = binary(home.path())
        .args(["--print-plan=full", "--workspace"])
        .arg(&work)
        .current_dir(&work)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("allow tcp example.com ports 443"));
    assert!(text.contains("planned publication: tcp ipv4 host:18000 -> sandbox:8000"));
    assert!(text.contains("network.dns-zero-ttl-grace-milliseconds = 750"));
    assert!(text.contains("network.udp-idle-timeout-seconds = 120"));
    assert!(text.contains("process.shutdown-grace-seconds = 9"));
}
