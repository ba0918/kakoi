use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;

use kakoi_runtime::{config, HostContext, ListMode, NetworkMode};

fn main() {
    if std::env::args_os().nth(1).as_deref() == Some("--self-test-input".as_ref()) {
        self_test_input();
    } else {
        panic!("this example currently verifies explicit configuration input only");
    }
}

fn self_test_input() {
    let cwd = std::env::current_dir().unwrap();
    let env: BTreeMap<_, _> = std::env::vars_os().collect();
    let context = HostContext::new(cwd.clone(), env.clone()).unwrap();
    assert_eq!(context.cwd(), cwd);
    assert_eq!(context.environment(), &env);
    assert!(HostContext::new(PathBuf::from("relative"), env.clone()).is_err());
    assert!(HostContext::new(
        PathBuf::from(OsString::from_vec(b"/with\0nul".to_vec())),
        env.clone()
    )
    .is_err());
    for name in ["", "A=B", "A\0B"] {
        let mut invalid = env.clone();
        invalid.insert(name.into(), "value".into());
        assert!(HostContext::new(cwd.clone(), invalid).is_err());
    }
    let mut invalid = env.clone();
    invalid.insert("NUL_VALUE".into(), OsString::from_vec(b"value\0".to_vec()));
    assert!(HostContext::new(cwd.clone(), invalid).is_err());
    let mut non_utf8 = env.clone();
    let name = OsString::from_vec(b"RAW_\xff".to_vec());
    let value = OsString::from_vec(b"VALUE_\xfe".to_vec());
    non_utf8.insert(name.clone(), value.clone());
    let raw = HostContext::new(cwd.clone(), non_utf8).unwrap();
    assert_eq!(raw.environment().get(&name), Some(&value));
    assert!(!format!("{raw:?}").contains("VALUE_"));
    let selection = config::Selection {
        profile: "explicit".into(),
        policy_file: Some("policy.toml".into()),
        rw: Vec::new(),
        hide: Vec::new(),
    };
    let loaded = config::load(&selection, &context).unwrap();
    assert_eq!(loaded.policy.mounts_mode(), ListMode::Host);
    assert_eq!(loaded.policy.network_mode(), NetworkMode::None);
    assert_eq!(loaded.sources.len(), 2);
    assert_eq!(std::env::current_dir().unwrap(), cwd);
    assert_eq!(std::env::vars_os().collect::<BTreeMap<_, _>>(), env);
    println!("explicit input validation succeeded");
}
