use super::*;

// Keep the existing filesystem regressions intact while sharing their real
// rename checkpoint with a separate, forcibly terminated child process.
#[path = "archive_set_tests.rs"]
mod archive_sets;
#[path = "tests_cases.rs"]
mod cases;
#[path = "cloud_acceptance_tests.rs"]
mod cloud_acceptance;
#[path = "process_recovery_tests.rs"]
mod process_recovery;

pub(super) fn checkpoint(index: usize) -> Result<()> {
    process_recovery::checkpoint(index)?;
    cases::checkpoint(index)
}
