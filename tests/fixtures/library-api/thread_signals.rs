mod signal_state;

fn main() {
    signal_state::install_application_state();
    let signals = signal_state::Signals::capture();
    let before = signal_state::raw_state();
    std::thread::spawn(|| {}).join().unwrap();
    signals.assert_preserved();
    signal_state::report(before, signals.usable);
}
