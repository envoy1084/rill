//! Confining a step's process on Linux: Landlock limits the filesystem to
//! read-only system directories and a writable work directory (and, when
//! the network is off, refuses TCP), seccomp refuses system calls no build
//! needs, and resource limits cap file sizes and core dumps.
//!
//! Everything that allocates is prepared in the server before `fork`;
//! the child only makes the final system calls before `exec`.

use std::path::{Path, PathBuf};

use landlock::{
    path_beneath_rules, Access, AccessFs, AccessNet, Ruleset, RulesetAttr, RulesetCreated,
    RulesetCreatedAttr, RulesetStatus, ABI,
};
use seccompiler::{BpfProgram, SeccompAction, SeccompFilter, TargetArch};

use crate::error::{Error, Result};

/// The newest Landlock ABI asked for; older kernels get what they support.
const LANDLOCK_ABI: ABI = ABI::V5;

/// Largest file a step may write.
const MAX_FILE_BYTES: u64 = 4 << 30;

/// System directories steps may read and execute from.
const SYSTEM_DIRS: &[&str] = &[
    "/usr", "/lib", "/lib64", "/lib32", "/bin", "/sbin", "/etc", "/opt", "/nix", "/proc", "/sys",
];

/// Calls refused with `EPERM`: none of them belongs in a build or test.
const DENIED_SYSCALLS: &[libc::c_long] = &[
    libc::SYS_ptrace,
    libc::SYS_mount,
    libc::SYS_umount2,
    libc::SYS_pivot_root,
    libc::SYS_setns,
    libc::SYS_unshare,
    libc::SYS_kexec_load,
    libc::SYS_bpf,
    libc::SYS_perf_event_open,
    libc::SYS_keyctl,
    libc::SYS_add_key,
    libc::SYS_request_key,
    libc::SYS_init_module,
    libc::SYS_finit_module,
    libc::SYS_delete_module,
    libc::SYS_reboot,
    libc::SYS_swapon,
    libc::SYS_swapoff,
    libc::SYS_settimeofday,
    libc::SYS_clock_settime,
    libc::SYS_acct,
];

/// A prepared confinement, applied once in the child.
pub struct Confinement {
    ruleset: Option<RulesetCreated>,
    filter: BpfProgram,
}

impl Confinement {
    /// Prepare to confine a step working in `dir`, reading the system and
    /// `extra_read` (its `PATH`), with TCP only when `network`.
    pub fn prepare(dir: &Path, extra_read: &[PathBuf], network: bool) -> Result<Self> {
        let fail = |what: &str, e: &dyn std::fmt::Display| Error::Sandbox(format!("{what}: {e}"));
        let read = AccessFs::from_read(LANDLOCK_ABI);
        let mut ruleset = Ruleset::default()
            .handle_access(AccessFs::from_all(LANDLOCK_ABI))
            .map_err(|e| fail("landlock", &e))?;
        if !network {
            ruleset = ruleset
                .handle_access(AccessNet::from_all(LANDLOCK_ABI))
                .map_err(|e| fail("landlock", &e))?;
        }
        let readable: Vec<PathBuf> = SYSTEM_DIRS
            .iter()
            .map(PathBuf::from)
            .chain(extra_read.iter().cloned())
            .filter(|p| p.exists())
            .collect();
        let ruleset = ruleset
            .create()
            .map_err(|e| fail("landlock", &e))?
            .add_rules(path_beneath_rules(&readable, read))
            .map_err(|e| fail("landlock", &e))?
            .add_rules(path_beneath_rules(["/dev"], read | AccessFs::WriteFile))
            .map_err(|e| fail("landlock", &e))?
            .add_rules(path_beneath_rules([dir], AccessFs::from_all(LANDLOCK_ABI)))
            .map_err(|e| fail("landlock", &e))?;

        let arch = TargetArch::try_from(std::env::consts::ARCH).map_err(|e| fail("seccomp", &e))?;
        let rules = DENIED_SYSCALLS.iter().map(|&s| (s, Vec::new())).collect();
        let filter = SeccompFilter::new(
            rules,
            SeccompAction::Allow,
            SeccompAction::Errno(libc::EPERM as u32),
            arch,
        )
        .map_err(|e| fail("seccomp", &e))?;
        let filter = BpfProgram::try_from(filter).map_err(|e| fail("seccomp", &e))?;
        Ok(Confinement {
            ruleset: Some(ruleset),
            filter,
        })
    }

    /// Apply to the calling process; run in the child between `fork` and
    /// `exec`.
    pub fn apply(&mut self) -> std::io::Result<()> {
        let limit = |resource, value: u64| {
            let rlim = libc::rlimit {
                rlim_cur: value,
                rlim_max: value,
            };
            // SAFETY: setrlimit reads a valid rlimit struct and only
            // affects this process.
            if unsafe { libc::setrlimit(resource, &rlim) } != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        };
        limit(libc::RLIMIT_FSIZE, MAX_FILE_BYTES)?;
        limit(libc::RLIMIT_CORE, 0)?;
        if let Some(ruleset) = self.ruleset.take() {
            let status = ruleset
                .restrict_self()
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            if status.ruleset == RulesetStatus::NotEnforced {
                return Err(std::io::Error::other("kernel does not enforce Landlock"));
            }
        }
        seccompiler::apply_filter(&self.filter).map_err(|e| std::io::Error::other(e.to_string()))
    }
}
