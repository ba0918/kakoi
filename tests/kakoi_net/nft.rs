use crate::common::TempDir;
use kakoi_net::{namespace::NetworkNamespace, nft};
use std::{
    path::Path,
    time::{Duration, Instant},
};

fn executable(home: &TempDir, script: &str) -> std::path::PathBuf {
    home.write_executable("nft", script)
}

// @kotowari[REQ-057]
#[test]
fn large_batch_and_error_output_do_not_deadlock_each_other() {
    let home = TempDir::new();
    let tool = executable(&home, "#!/usr/bin/python3\nimport sys\nsys.stderr.write('x' * 100000)\nassert len(sys.stdin.buffer.read()) == 2000000\nsys.stderr.write('batch rejected')\nsys.exit(7)\n");
    let namespace = NetworkNamespace::create().unwrap();
    let error = nft::apply(
        &namespace,
        &tool,
        &"x".repeat(2000000),
        Instant::now() + Duration::from_secs(3),
    )
    .unwrap_err();
    assert!(error.to_string().contains("batch rejected"));
    assert!(error.to_string().len() < 10000);
}

// @kotowari[REQ-057]
#[test]
fn blocked_batch_write_obeys_the_deadline_and_reaps_the_child() {
    let home = TempDir::new();
    let marker = home.path().join("pid");
    let tool = executable(
        &home,
        &format!(
            "#!/bin/sh\necho $$ > '{}'\nexec /bin/sleep 30\n",
            marker.display()
        ),
    );
    let namespace = NetworkNamespace::create().unwrap();
    let error = nft::apply(
        &namespace,
        &tool,
        &"x".repeat(2000000),
        Instant::now() + Duration::from_millis(200),
    )
    .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    let pid = std::fs::read_to_string(marker).unwrap();
    assert!(!Path::new(&format!("/proc/{}", pid.trim())).exists());
}

// @kotowari[REQ-057]
#[test]
fn rejected_kernel_batch_does_not_partially_install_rules() {
    let namespace = NetworkNamespace::create().unwrap();
    let tool = Path::new("/usr/sbin/nft");
    nft::apply(
        &namespace,
        tool,
        "add table inet baseline\n",
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    assert!(nft::apply(
        &namespace,
        tool,
        "add table inet partial\nadd rule inet missing output drop\n",
        Instant::now() + Duration::from_secs(2)
    )
    .is_err());
    let output = namespace
        .command(tool)
        .unwrap()
        .args(["list", "tables"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "table inet baseline\n"
    );
}

// @kotowari[REQ-014, REQ-389]
#[test]
fn kernel_readback_exposes_the_actual_finite_element_expiration() {
    let namespace = kakoi_net::namespace::NetworkNamespace::create().unwrap();
    let executable = std::path::Path::new("/usr/sbin/nft");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    kakoi_net::nft::apply(&namespace, executable, "add table inet readback\nadd set inet readback addresses { type ipv4_addr; flags timeout; }\nadd element inet readback addresses { 1.1.1.1 timeout 1s }\n", deadline).unwrap();
    let json = kakoi_net::nft::inspect(&namespace, executable, "readback", deadline).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
    let sets = value["nftables"].as_array().unwrap();
    let set = sets.iter().find_map(|entry| entry.get("set")).unwrap();
    let expires = set["elem"][0]["elem"]["expires"].as_u64().unwrap();
    assert!(expires <= 1, "{json:?}");
}
