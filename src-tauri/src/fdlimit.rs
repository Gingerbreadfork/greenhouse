//! The open-file limit: raised for the engine's peer connections, and put back
//! for the programs Greenhouse launches directly.

/// The soft and hard limits Greenhouse started with.
#[cfg(unix)]
static STARTING: std::sync::OnceLock<(u64, u64)> = std::sync::OnceLock::new();

/// Raises the open-file soft limit as far as the system allows.
pub fn raise() {
    #[cfg(unix)]
    if let Ok(limits) = rlimit::getrlimit(rlimit::Resource::NOFILE) {
        let _ = STARTING.set(limits);
    }
    if let Err(e) = librqbit::try_increase_nofile_limit() {
        eprintln!("Could not raise the open file limit: {e:#}");
    }
}

/// The open-file soft limit in force, where the system has one.
pub fn current() -> Option<u64> {
    #[cfg(unix)]
    {
        rlimit::getrlimit(rlimit::Resource::NOFILE).ok().map(|(soft, _)| soft)
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// Makes a program start with the open-file limits Greenhouse started with.
#[cfg(unix)]
pub fn restore(command: &mut std::process::Command) {
    if let Some(&(soft, hard)) = STARTING.get() {
        limit(command, soft, hard);
    }
}

/// Makes a program start with the given open-file limits.
#[cfg(unix)]
fn limit(command: &mut std::process::Command, soft: u64, hard: u64) {
    use std::os::unix::process::CommandExt;
    unsafe {
        command.pre_exec(move || rlimit::setrlimit(rlimit::Resource::NOFILE, soft, hard));
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_started_program_gets_the_open_file_limit_it_is_given() {
        let (soft, hard) = rlimit::getrlimit(rlimit::Resource::NOFILE).unwrap();
        let mut command = std::process::Command::new("sh");
        command.args(["-c", "ulimit -Sn"]);
        limit(&mut command, soft / 2, hard);

        let out = command.output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), (soft / 2).to_string());
        assert_eq!(rlimit::getrlimit(rlimit::Resource::NOFILE).unwrap(), (soft, hard));
    }
}
