//! Worker hardening (docs/spec.md §10.2): after the handshake, a seccomp-BPF
//! filter limits every thread of the worker to the system calls the worker
//! loop, the engine's allocator and Rust's runtime need. The engine makes no
//! system calls of its own, so an engine bug can at worst kill the worker.
//!
//! Allowed (measured with strace over plak's worker suites, see
//! docs/research/plak-integration.md):
//!
//! - `read` on stdin; `write` on stdout and stderr (and a test hook's file);
//!   `pread64` on the `/proc/self/status` descriptor opened before the
//!   filter, for the resource report;
//! - memory: `brk`, `mmap` and `mprotect` without `PROT_EXEC`, `munmap`,
//!   `mremap`, `madvise`;
//! - threads (the stdin reader and the cancel repeater): `clone` with
//!   `CLONE_THREAD` only (`clone3` fails with `ENOSYS`, so glibc falls back
//!   to `clone`, whose flags a filter can read), `set_robust_list`, `rseq`,
//!   `prctl(PR_SET_NAME)`, `sched_getaffinity`, `sigaltstack`,
//!   `rt_sigprocmask`, `rt_sigreturn`, `futex`, `sched_yield`, `exit`;
//! - time and entropy: `clock_gettime`, `clock_nanosleep`, `nanosleep`
//!   (the cancel repeater's sleep), `getrandom` (std's hash keys and the
//!   engine's default entropy for `random`/`os.urandom`/`uuid`);
//! - the end: `getrusage` (resource report), `exit_group`, and what
//!   `abort` needs to raise `SIGABRT` on itself (`getpid`, `gettid`,
//!   `rt_sigaction`, `tgkill` aimed at this process).
//!
//! `openat` (and `open`) fail with `EACCES`: glibc's malloc reads
//! `/proc/sys/vm/overcommit_memory` the first time it shrinks a thread
//! arena, at any point of a run, and takes a failed open as "do not
//! unmap". No call that creates a descriptor succeeds.
//!
//! Anything else kills the whole process (`SECCOMP_RET_KILL_PROCESS`) rather
//! than failing with an errno: nothing in the worker is meant to reach
//! another system call, so one that does is a bug or an exploit, and a dead
//! worker is a clean, typed loss for plak (the parent sees the stream end
//! without a terminal) where a failing call would let a confused process
//! continue. `MM_CELL_WORKER_NO_SECCOMP=1` skips the filter for debugging.
//!
//! Installing a filter needs `prctl` and `seccomp`, which only exist as
//! `unsafe` FFI; this module is the worker's one exception to the
//! workspace's `unsafe_code` lint (docs/spec.md §5).

#![allow(unsafe_code, reason = "seccomp and getrusage FFI; see the module docs")]

use std::{fs::File, os::unix::fs::FileExt, os::unix::io::AsRawFd};

use plakous_cell::ProcessResourceUsage;

/// Set to `1` to run without the filter (debugging, strace).
pub(crate) const OPT_OUT: &str = "MM_CELL_WORKER_NO_SECCOMP";

#[cfg(target_arch = "x86_64")]
const AUDIT_ARCH: u32 = 0xC000_003E;
#[cfg(target_arch = "aarch64")]
const AUDIT_ARCH: u32 = 0xC000_00B7;

const PROT_EXEC: u32 = 4;
const CLONE_THREAD: u32 = 0x0001_0000;
const PR_SET_NAME: u32 = 15;
const ENOSYS: u32 = 38;
const EACCES: u32 = 13;

/// What the filter does with one system call.
enum Check {
    Allow,
    /// Fails with this errno.
    Errno(u32),
    /// Allowed when argument `.0`'s low 32 bits are one of `.1`.
    ArgIn(u32, Vec<u32>),
    /// Allowed when argument `.0` has none of the bits `.1`.
    ArgClear(u32, u32),
    /// Allowed when argument `.0` has all of the bits `.1`.
    ArgSet(u32, u32),
}

// Classic BPF opcodes.
const LD_W_ABS: u16 = 0x20;
const JEQ_K: u16 = 0x15;
const JGE_K: u16 = 0x35;
const JSET_K: u16 = 0x45;
const RET_K: u16 = 0x06;
const RET_ALLOW: u32 = 0x7fff_0000;
const RET_KILL: u32 = 0x8000_0000;
const RET_ERRNO: u32 = 0x0005_0000;
// `struct seccomp_data`: nr, arch, instruction_pointer, args[6].
const OFF_NR: u32 = 0;
const OFF_ARCH: u32 = 4;
const OFF_ARGS: u32 = 16;

fn op(code: u16, jt: u8, jf: u8, k: u32) -> libc::sock_filter {
    libc::sock_filter { code, jt, jf, k }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "syscall numbers are small and non-negative"
)]
fn nr(n: libc::c_long) -> u32 {
    n as u32
}

/// The allowlist; `out_fds` may be written, `status_fd` read with `pread64`.
fn rules(out_fds: &[u32], status_fd: u32) -> Vec<(u32, Check)> {
    use Check::{Allow, ArgClear, ArgIn, ArgSet, Errno};
    let mut writable = vec![1, 2];
    writable.extend_from_slice(out_fds);
    vec![
        (nr(libc::SYS_futex), Allow),
        (nr(libc::SYS_read), ArgIn(0, vec![0])),
        (nr(libc::SYS_write), ArgIn(0, writable)),
        (nr(libc::SYS_mmap), ArgClear(2, PROT_EXEC)),
        (nr(libc::SYS_mprotect), ArgClear(2, PROT_EXEC)),
        (nr(libc::SYS_munmap), Allow),
        (nr(libc::SYS_mremap), Allow),
        (nr(libc::SYS_madvise), Allow),
        (nr(libc::SYS_brk), Allow),
        (nr(libc::SYS_clock_gettime), Allow),
        (nr(libc::SYS_clock_nanosleep), Allow),
        (nr(libc::SYS_nanosleep), Allow),
        (nr(libc::SYS_sched_yield), Allow),
        (nr(libc::SYS_getrandom), Allow),
        (nr(libc::SYS_rt_sigprocmask), Allow),
        (nr(libc::SYS_rt_sigreturn), Allow),
        (nr(libc::SYS_sigaltstack), Allow),
        (nr(libc::SYS_clone), ArgSet(0, CLONE_THREAD)),
        (nr(libc::SYS_clone3), Errno(ENOSYS)),
        (nr(libc::SYS_openat), Errno(EACCES)),
        #[cfg(target_arch = "x86_64")]
        (nr(libc::SYS_open), Errno(EACCES)),
        (nr(libc::SYS_set_robust_list), Allow),
        (nr(libc::SYS_rseq), Allow),
        (nr(libc::SYS_prctl), ArgIn(0, vec![PR_SET_NAME])),
        (nr(libc::SYS_sched_getaffinity), Allow),
        (nr(libc::SYS_gettid), Allow),
        (nr(libc::SYS_getpid), Allow),
        (nr(libc::SYS_rt_sigaction), Allow),
        (nr(libc::SYS_tgkill), ArgIn(0, vec![std::process::id()])),
        (nr(libc::SYS_pread64), ArgIn(0, vec![status_fd])),
        (nr(libc::SYS_getrusage), Allow),
        (nr(libc::SYS_exit), Allow),
        (nr(libc::SYS_exit_group), Allow),
    ]
}

/// Compiles the allowlist to classic BPF: check the architecture, then one
/// block per system call; no match kills.
fn program(rules: &[(u32, Check)]) -> Vec<libc::sock_filter> {
    let mut code = vec![
        op(LD_W_ABS, 0, 0, OFF_ARCH),
        op(JEQ_K, 1, 0, AUDIT_ARCH),
        op(RET_K, 0, 0, RET_KILL),
        op(LD_W_ABS, 0, 0, OFF_NR),
    ];
    // x32 system calls (bit 30) on x86_64 would bypass the numbers below.
    #[cfg(target_arch = "x86_64")]
    code.extend([op(JGE_K, 0, 1, 0x4000_0000), op(RET_K, 0, 0, RET_KILL)]);
    for (number, check) in rules {
        let arg = |index: u32| op(LD_W_ABS, 0, 0, OFF_ARGS + 8 * index);
        let body = match check {
            Check::Allow => vec![op(RET_K, 0, 0, RET_ALLOW)],
            Check::Errno(errno) => vec![op(RET_K, 0, 0, RET_ERRNO | errno)],
            Check::ArgIn(index, values) => {
                let mut body = vec![arg(*index)];
                let n = values.len();
                for (i, value) in values.iter().enumerate() {
                    let to_allow = u8::try_from(n - i).unwrap_or(u8::MAX);
                    body.push(op(JEQ_K, to_allow, 0, *value));
                }
                body.extend([op(RET_K, 0, 0, RET_KILL), op(RET_K, 0, 0, RET_ALLOW)]);
                body
            }
            Check::ArgClear(index, bits) => vec![
                arg(*index),
                op(JSET_K, 0, 1, *bits),
                op(RET_K, 0, 0, RET_KILL),
                op(RET_K, 0, 0, RET_ALLOW),
            ],
            Check::ArgSet(index, bits) => vec![
                arg(*index),
                op(JSET_K, 1, 0, *bits),
                op(RET_K, 0, 0, RET_KILL),
                op(RET_K, 0, 0, RET_ALLOW),
            ],
        };
        let skip = u8::try_from(body.len()).unwrap_or(u8::MAX);
        code.push(op(JEQ_K, 0, skip, *number));
        code.extend(body);
    }
    code.push(op(RET_K, 0, 0, RET_KILL));
    code
}

/// What the resource report reads once the filter forbids opening files.
pub(crate) struct Report {
    /// Never closed: the filter allows no `close` (nor the `fcntl` check
    /// debug builds make before one).
    status: Option<&'static File>,
    fds: Option<u64>,
}

impl Report {
    /// Opens `/proc/self/status` and counts open descriptors. The count
    /// cannot change under the filter, which allows no call that makes one.
    pub(crate) fn prepare() -> Report {
        let status = File::open("/proc/self/status")
            .ok()
            .map(|file| &*Box::leak(Box::new(file)));
        let fds = std::fs::read_dir("/proc/self/fd")
            .ok()
            .and_then(|entries| u64::try_from(entries.count()).ok());
        Report { status, fds }
    }

    /// The report plak's `sample_self_rusage` makes, without opening files.
    pub(crate) fn sample(&self) -> ProcessResourceUsage {
        let mut usage = ProcessResourceUsage::default();
        // SAFETY: `getrusage` writes one `rusage` into the zeroed struct
        // it is given and reads nothing else.
        let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, &raw mut ru) } == 0 {
            let nanos = |tv: libc::timeval| {
                u64::try_from(tv.tv_sec)
                    .unwrap_or(0)
                    .saturating_mul(1_000_000_000)
                    .saturating_add(u64::try_from(tv.tv_usec).unwrap_or(0).saturating_mul(1000))
            };
            usage.user_cpu_ns = nanos(ru.ru_utime);
            usage.system_cpu_ns = nanos(ru.ru_stime);
            let max_rss = u64::try_from(ru.ru_maxrss).unwrap_or(0);
            usage.peak_rss_bytes = (max_rss > 0).then(|| max_rss.saturating_mul(1024));
        }
        if let Some(status) = self.status {
            let mut text = vec![0u8; 4096];
            let mut len = 0;
            while len < text.len() {
                match status.read_at(&mut text[len..], len as u64) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => len += n,
                }
            }
            for line in String::from_utf8_lossy(&text[..len]).lines() {
                let mut parts = line.split_whitespace();
                let (Some(key), Some(Ok(value))) =
                    (parts.next(), parts.next().map(str::parse::<u64>))
                else {
                    continue;
                };
                match key {
                    "VmRSS:" if value > 0 => usage.rss_bytes = Some(value.saturating_mul(1024)),
                    "Threads:" if value > 0 => usage.threads = Some(value),
                    _ => {}
                }
            }
        }
        usage.fds = self.fds;
        usage
    }

    fn status_fd(&self) -> u32 {
        self.status
            .and_then(|f| u32::try_from(f.as_raw_fd()).ok())
            .unwrap_or(u32::MAX)
    }
}

/// Installs the filter on every thread of the process (`TSYNC`), unless
/// [`OPT_OUT`] is `1`. `out_fds` are extra descriptors that may be written.
///
/// # Errors
///
/// Why the kernel refused it.
pub(crate) fn install(report: &Report, out_fds: &[u32]) -> Result<(), String> {
    if std::env::var(OPT_OUT).as_deref() == Ok("1") {
        return Ok(());
    }
    let mut code = program(&rules(out_fds, report.status_fd()));
    let prog = libc::sock_fprog {
        len: u16::try_from(code.len()).map_err(|_| "seccomp filter too long")?,
        filter: code.as_mut_ptr(),
    };
    // SAFETY: plain integer arguments; sets the calling thread's
    // no_new_privs bit, required to install a filter unprivileged.
    if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
        return Err(format!(
            "prctl(PR_SET_NO_NEW_PRIVS): {}",
            std::io::Error::last_os_error()
        ));
    }
    // SAFETY: `prog` points at `code`, which outlives the call; the kernel
    // copies the program.
    let installed = unsafe {
        libc::syscall(
            libc::SYS_seccomp,
            libc::SECCOMP_SET_MODE_FILTER,
            libc::SECCOMP_FILTER_FLAG_TSYNC,
            &raw const prog,
        )
    };
    if installed != 0 {
        return Err(format!(
            "seccomp(SECCOMP_SET_MODE_FILTER): {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::panic, reason = "tests")]

    use std::process::Command;

    /// The child half of [`a_forbidden_call_kills_the_process`]: installs
    /// the filter, proves allowed work still runs, that opening a file
    /// fails, and that creating a socket kills it.
    #[test]
    fn child_under_filter() {
        if std::env::var("MM_SECCOMP_TEST_CHILD").as_deref() != Ok("1") {
            return;
        }
        let report = super::Report::prepare();
        super::install(&report, &[]).expect("installed");
        // Allowed: allocation, threads, sleeping, writing, the report.
        let big = vec![7u8; 64 << 20];
        let joined = std::thread::Builder::new()
            .name("child".to_owned())
            .spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(1));
                big.iter().map(|&b| u64::from(b)).sum::<u64>()
            })
            .expect("thread")
            .join()
            .expect("joined");
        let usage = report.sample();
        println!(
            "SURVIVED {joined} {:?} {:?}",
            usage.threads, usage.rss_bytes
        );
        // Opening a file fails.
        let opened = std::fs::File::open("/dev/null").map_err(|e| e.kind());
        println!("OPEN {opened:?}");
        // Anything else that is not allowed kills the process.
        let socket = std::net::UdpSocket::bind("127.0.0.1:0");
        println!("SOCKET {}", socket.is_ok());
    }

    #[test]
    fn a_forbidden_call_kills_the_process() {
        use std::os::unix::process::ExitStatusExt;
        let output = Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--exact",
                "seccomp::tests::child_under_filter",
                "--nocapture",
            ])
            .args(["--test-threads", "1"])
            .env("MM_SECCOMP_TEST_CHILD", "1")
            .env_remove(super::OPT_OUT)
            .output()
            .expect("child runs");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("SURVIVED 469762048 Some(") && stdout.contains(") Some("),
            "{stdout}"
        );
        assert!(stdout.contains("OPEN Err(PermissionDenied)"), "{stdout}");
        assert!(!stdout.contains("SOCKET"), "{stdout}");
        assert_eq!(output.status.signal(), Some(libc::SIGSYS), "{output:?}");
    }
}
