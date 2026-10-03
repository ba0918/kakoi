//! CLI role detection and guard execution share the runtime implementation.
pub fn run_if_guard() -> Option<std::process::ExitCode> {
    kakoi_runtime::cli::run_if_guard()
}
