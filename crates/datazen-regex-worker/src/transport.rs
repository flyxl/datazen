use std::{
    io::{self, Read, Write},
    net::Shutdown,
    os::{fd::OwnedFd, unix::{net::UnixStream, process::CommandExt}},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::{Error, Outcome, Request, WORKER_ARG, isolation, protocol};

const MAX_DURATION: Duration = Duration::from_secs(1);
const WAIT: Duration = Duration::from_millis(1);

/// Use on a single dedicated background worker, not the render thread. No
/// reply can be successful after the deadline. Rust's spawn/reap syscalls
/// themselves are not interruptible from this loop (e.g. a kernel I/O stall).
/// Killing a running user-space regex does not wait for engine cooperation.
pub fn search(request: Request<'_>, timeout: Duration) -> Result<Outcome, Error> {
    let deadline = Instant::now() + timeout.min(MAX_DURATION);
    let encoded = protocol::encode_request(&request)?;
    let mut wire = Vec::with_capacity(encoded.len() + 4);
    protocol::write_frame(&mut wire, &encoded).map_err(|_| Error::Unavailable)?;
    let mut link = ChildLink::launch(deadline)?;
    let reply = link.transfer(&wire, deadline)?;
    let result = protocol::decode_reply(&request, &reply);
    until(deadline)?;
    result
}

struct KillAndReap(Child);

impl Drop for KillAndReap {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            // Before sandbox setup only the trusted executable runs. After
            // it reports ready, its seccomp policy forbids spawning.
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

struct ChildLink {
    parent: UnixStream,
    child: KillAndReap,
}

impl ChildLink {
    fn launch(deadline: Instant) -> Result<Self, Error> {
        let fail = |_| Error::Unavailable;
        let (parent, child) = UnixStream::pair().map_err(fail)?;
        parent.set_nonblocking(true).map_err(fail)?;
        let input: OwnedFd = child.try_clone().map_err(fail)?.into();
        let output: OwnedFd = child.into();
        // The kernel resolves /proc/self/exe in the forked process before
        // exec, using the current application inode, not PATH or argv.
        let mut command = Command::new("/proc/self/exe");
        command
            .arg(WORKER_ARG)
            .env_clear()
            .current_dir("/")
            .stdin(Stdio::from(input))
            .stdout(Stdio::from(output))
            .stderr(Stdio::null());
        // SAFETY: before_exec performs only scalar syscalls, neither
        // allocating nor accessing userspace locks in the forked process.
        let parent_pid = std::process::id() as libc::pid_t;
        unsafe { command.pre_exec(move || isolation::before_exec(parent_pid)); }
        until(deadline)?;
        let child = KillAndReap(command.spawn().map_err(fail)?);
        drop(command);
        until(deadline)?;
        Ok(Self { parent, child })
    }

    fn transfer(&mut self, request: &[u8], deadline: Instant) -> Result<Vec<u8>, Error> {
        // A failed kernel restriction must never result in transmitting the
        // user's document to a running, unrestricted matching process.
        let mut ready = [0u8; 1];
        loop {
            until(deadline)?;
            match self.parent.read(&mut ready) {
                Ok(1) if ready[0] == protocol::READY => break,
                Err(e) if retry(&e) => wait(deadline),
                _ => return Err(Error::Unavailable),
            }
        }

        let mut sent = 0;
        let mut shut = false;
        let mut eof = false;
        let mut reply = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            until(deadline)?;
            let mut progressed = false;
            if sent < request.len() {
                match self.parent.write(&request[sent..]) {
                    Ok(0) => return Err(Error::Unavailable),
                    Ok(n) => { sent += n; progressed = true; }
                    Err(e) if retry(&e) => {}
                    Err(_) => return Err(Error::Unavailable),
                }
            }
            if sent == request.len() && !shut {
                self.parent.shutdown(Shutdown::Write).map_err(|_| Error::Unavailable)?;
                shut = true;
            }
            if !eof {
                match self.parent.read(&mut chunk) {
                    Ok(0) => eof = true,
                    Ok(n) => {
                        if reply.len() + n > protocol::MAX_REPLY + 4 {
                            return Err(Error::InvalidReply);
                        }
                        reply.extend_from_slice(&chunk[..n]);
                        if let Some(len) = length(&reply)
                            && (len == 0 || len > protocol::MAX_REPLY || reply.len() > len + 4)
                        {
                            return Err(Error::InvalidReply);
                        }
                        progressed = true;
                    }
                    Err(e) if retry(&e) => {}
                    Err(_) => return Err(Error::Unavailable),
                }
            }
            if eof && let Some(status) = self.child.0.try_wait().map_err(|_| Error::Unavailable)? {
                until(deadline)?;
                return if status.success() && shut && length(&reply) == reply.len().checked_sub(4) {
                    // A zero-length or truncated prefix cannot be valid.
                    if reply.len() < 5 { Err(Error::InvalidReply) } else { Ok(reply.split_off(4)) }
                } else {
                    Err(Error::InvalidReply)
                };
            }
            if !progressed {
                wait(deadline);
            }
        }
    }
}

fn until(deadline: Instant) -> Result<(), Error> {
    if Instant::now() < deadline { Ok(()) } else { Err(Error::TimedOut) }
}

fn length(reply: &[u8]) -> Option<usize> {
    protocol::wire_size(reply.get(..4)?)
}

fn retry(error: &io::Error) -> bool {
    matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted)
}

fn wait(deadline: Instant) {
    thread::sleep(WAIT.min(deadline.saturating_duration_since(Instant::now())));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_reply_times_out_and_child_is_reaped() {
        // A deliberately stalled synthetic peer holds the descriptor; no
        // heavy pattern, actual user data, or timing assumptions about Regress.
        let (parent, mut fake_peer) = UnixStream::pair().unwrap();
        parent.set_nonblocking(true).unwrap();
        let child = Command::new("/bin/sleep")
            .arg("5")
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id() as libc::pid_t;
        fake_peer.write_all(&[protocol::READY, 7, 0, 0, 0, 1]).unwrap();
        let mut link = ChildLink { parent, child: KillAndReap(child) };
        assert_eq!(
            link.transfer(&[0], Instant::now() + Duration::from_millis(30)),
            Err(Error::TimedOut)
        );
        drop(link);
        let mut status = 0;
        // SAFETY: test-only check for receipt of the already reaped child.
        assert_eq!(unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) }, -1);
        assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::ECHILD));
    }

    #[test]
    fn oversized_frame_is_rejected_before_accumulation() {
        let (parent, mut fake_peer) = UnixStream::pair().unwrap();
        parent.set_nonblocking(true).unwrap();
        let child = Command::new("/bin/sleep").arg("5").env_clear().spawn().unwrap();
        fake_peer.write_all(&[protocol::READY]).unwrap();
        fake_peer.write_all(&((protocol::MAX_REPLY as u32) + 1).to_le_bytes()).unwrap();
        let mut link = ChildLink { parent, child: KillAndReap(child) };
        assert_eq!(
            link.transfer(&[0], Instant::now() + Duration::from_secs(1)),
            Err(Error::InvalidReply)
        );
    }
}
