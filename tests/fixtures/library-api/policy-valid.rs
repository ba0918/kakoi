use kakoi_policy::{EnvironmentPolicy, EnvMode, ListMode, MountPolicy, NetworkMode, NetworkPolicy, Policy, PolicyInput};
fn main() {
    let input = PolicyInput::new(MountPolicy::new(ListMode::Host), NetworkPolicy::new(NetworkMode::None), EnvironmentPolicy::new(EnvMode::Clear));
    Policy::validate(input).unwrap();
}
