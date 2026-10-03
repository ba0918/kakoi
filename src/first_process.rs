//! CLI dispatch to the shared runtime's exec-only listed helper.
pub fn run_if_first_process() -> Option<std::process::ExitCode> {
    kakoi_runtime::cli::run_if_first_process()
}
