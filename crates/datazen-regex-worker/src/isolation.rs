//! Deliberately Linux x86_64 only. The child is a fresh exec of the trusted
//! current image. No document bytes are delivered before this setup succeeds.
use std::io;

const AUDIT_ARCH_X86_64: u32 = 0xc000_003e;
const SECCOMP_NR: u32 = 0;
const SECCOMP_ARCH: u32 = 4;

/// Runs in the post-fork/pre-exec path; no locks, heap, environment or I/O.
pub(crate) fn before_exec(parent: libc::pid_t) -> io::Result<()> {
    // SAFETY: close_range gets integer arguments. CLOEXEC preserves std's
    // spawn error channel until successful exec. No app descriptor survives.
    checked(unsafe {
        libc::syscall(
            libc::SYS_close_range,
            3u32,
            u32::MAX,
            libc::CLOSE_RANGE_CLOEXEC,
        )
    })?;
    // SAFETY: prctl receives the documented scalar arguments.
    checked(unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) })?;
    // SAFETY: scalar prctl and getppid. Check the race where the parent died
    // before the signal was installed.
    checked(unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0) })?;
    if unsafe { libc::getppid() } != parent {
        return Err(io::Error::from_raw_os_error(libc::ECHILD));
    }
    Ok(())
}

/// Called before dispatching any user-supplied input. Leave the full Native
/// image's dynamic loader outside the memory cap; limit matching, not startup.
pub(crate) fn confine() -> io::Result<()> {
    for (kind, value) in [
        (libc::RLIMIT_AS, 384 * 1024 * 1024),
        (libc::RLIMIT_STACK, 8 * 1024 * 1024),
        (libc::RLIMIT_CORE, 0),
        (libc::RLIMIT_CPU, 1),
        (libc::RLIMIT_NOFILE, 3),
    ] {
        let limit = libc::rlimit { rlim_cur: value, rlim_max: value };
        // SAFETY: limit points to the correctly initialized ABI struct.
        checked(unsafe { libc::setrlimit(kind, &limit) })?;
    }
    // SAFETY: this also protects someone invoking the hidden child argument
    // directly, independent of whether the trusted pre-exec ran.
    checked(unsafe { libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, 0u32) })?;
    // SAFETY: only scalar arguments, unrelated to user input.
    checked(unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) })?;

    // A piped core-dump handler may ignore RLIMIT_CORE on Linux. Also mark
    // the worker non-dumpable before documents enter its address space.
    // SAFETY: scalar flag, documented by PR_SET_DUMPABLE.
    checked(unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) })?;

    // Linux seccomp_data{nr:i32,arch:u32,...}. The architecture test excludes
    // other tables; x32 sets a high bit and matches none of these exact
    // syscall numbers. There is no socket, open, file mutation, process
    // creation, signalling, ptrace, ioctl, clone, or exec syscall.
    let mut filter = vec![
        load(SECCOMP_ARCH),
        skip_equal(AUDIT_ARCH_X86_64, 1),
        ret(libc::SECCOMP_RET_KILL_PROCESS),
        load(SECCOMP_NR),
    ];
    for syscall in [
        libc::SYS_read,
        libc::SYS_write,
        libc::SYS_writev,
        libc::SYS_brk,
        libc::SYS_mmap,
        libc::SYS_munmap,
        libc::SYS_mremap,
        libc::SYS_madvise,
        libc::SYS_futex,
        libc::SYS_getrandom,
        libc::SYS_sigaltstack,
        libc::SYS_rt_sigprocmask,
        libc::SYS_clock_gettime,
        libc::SYS_rt_sigreturn,
        libc::SYS_exit_group,
        libc::SYS_exit,
    ] {
        filter.push(skip_equal(syscall as u32, 0));
        filter.push(ret(libc::SECCOMP_RET_ALLOW));
    }
    filter.push(ret(libc::SECCOMP_RET_KILL_PROCESS));
    let filter = libc::sock_fprog {
        len: u16::try_from(filter.len()).map_err(io::Error::other)?,
        filter: filter.as_mut_ptr(),
    };
    // SAFETY: the kernel copies the initialized BPF sequence synchronously;
    // its pointer is live throughout this syscall. TSYNC fails closed if the
    // filter cannot be loaded into every thread in this process.
    let result = unsafe {
        libc::syscall(
            libc::SYS_seccomp,
            libc::SECCOMP_SET_MODE_FILTER,
            libc::SECCOMP_FILTER_FLAG_TSYNC,
            &filter as *const libc::sock_fprog,
        )
    };
    if result != 0 {
        return Err(io::Error::other("could not confine regex worker"));
    }
    Ok(())
}

fn checked(result: impl Into<i64>) -> io::Result<()> {
    if result.into() < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn load(offset: u32) -> libc::sock_filter {
    instruction(libc::BPF_LD | libc::BPF_W | libc::BPF_ABS, 0, 0, offset)
}

fn skip_equal(value: u32, yes: u8) -> libc::sock_filter {
    instruction(libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K, yes, 1 - yes, value)
}

fn ret(action: u32) -> libc::sock_filter {
    instruction(libc::BPF_RET | libc::BPF_K, 0, 0, action)
}

fn instruction(op: u32, jt: u8, jf: u8, k: u32) -> libc::sock_filter {
    libc::sock_filter { code: op as u16, jt, jf, k }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::{CommandExt, ExitStatusExt};
    use std::process::{Command, Stdio};

    const VAR: &str = "DATAZEN_CONFINED_TEST";
    const NAME: &str = "isolation::tests::worker_probe";

    #[test]
    fn worker_probe() {
        let Some(probe) = std::env::var_os(VAR) else { return };
        // SAFETY: test-only fgets-free synthetic FD probe, no real data.
        assert_eq!(unsafe { libc::fcntl(90, libc::F_GETFD) }, -1);
        confine().unwrap();
        // SAFETY: only fixed synthetic no-data probes, in a disposable process.
        unsafe {
            match probe.to_str() {
                Some("read-device") => { libc::open(c"/dev/null".as_ptr(), libc::O_RDONLY); }
                Some("socket") => { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0); }
                Some("fork") => {
                    if libc::fork() == 0 { libc::_exit(70); }
                }
                _ => libc::_exit(71),
            }
            libc::_exit(72); // none of these calls may return
        }
    }

    #[test]
    fn closes_inherited_descriptors_and_kernel_denies_open_socket_fork() {
        let image = std::env::current_exe().unwrap();
        for probe in ["read-device", "socket", "fork"] {
            let mut command = Command::new(&image);
            command.args(["--exact", NAME]).env_clear().env(VAR, probe)
                .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
            let parent = std::process::id() as libc::pid_t;
            // SAFETY: install a synthetic inheritable FD and apply the actual
            // worker launch restrictions before harness exec.
            unsafe { command.pre_exec(move || {
                if libc::dup2(0, 90) < 0 { return Err(io::Error::last_os_error()); }
                before_exec(parent)
            }); }
            let status = command.status().unwrap();
            assert_eq!(status.signal(), Some(libc::SIGSYS), "{probe} {status}");
        }
    }
}
