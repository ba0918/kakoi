use kakoi_policy::{
    EnvMode, EnvironmentPolicy, ListMode, MountPolicy, NetworkMode, NetworkPolicy, Policy,
    PolicyInput,
};

fn main() {
    let rust = Policy::validate(PolicyInput::new(
        MountPolicy::new(ListMode::Host),
        NetworkPolicy::new(NetworkMode::None),
        EnvironmentPolicy::new(EnvMode::Clear),
    ))
    .unwrap();
    let toml = Policy::from_toml("[network]\nmode='none'\n[env]\nmode='clear'").unwrap();
    assert_eq!(rust.mounts_mode(), toml.mounts_mode());
    assert_eq!(rust.network_mode(), toml.network_mode());
    assert_eq!(rust.environment_mode(), toml.environment_mode());
    println!("policy validation succeeded");
}
