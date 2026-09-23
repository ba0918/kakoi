//! The exit code of a filtered run, decided apart from the processes it describes.

/// Code for an environment ended because neither enforcement nor blocking held.
pub const SAFETY_FAULT: u8 = 125;
/// Code for an external termination request received while the main command ran.
pub const TERMINATED: u8 = 143;

/// `main` is the confirmed result of the main command (`128 + signal` when a
/// signal ended it). A termination request after that result keeps it.
pub fn exit_code(safety_fault: bool, terminated_while_running: bool, main: u8) -> u8 {
    if safety_fault {
        SAFETY_FAULT
    } else if terminated_while_running {
        TERMINATED
    } else {
        main
    }
}
