//! Virtual terminal handling.
//!
//! ndlm drives the display through DRM, which means the scanout keeps showing
//! the login screen even while the user is on another VT: the kernel console can
//! only repaint a VT once nobody holds DRM master, and it never retries on its
//! own. To stay on the VT ndlm was started on, the greeter uses the same
//! handshake as X and Wayland compositors:
//!
//! * the VT is put into `VT_PROCESS` mode, so the kernel asks for permission
//!   (SIGUSR1) before switching away and reports (SIGUSR2) that we are active
//!   again,
//! * on SIGUSR1 the display is handed back: drop DRM master and only then
//!   release the switch with `VT_RELDISP(1)`, so the VT being switched to can
//!   paint itself (the kernel console cannot take over while we hold master),
//! * on SIGUSR2 the switch is acknowledged with `VT_RELDISP(VT_ACKACQ)` and the
//!   display is taken back: `KD_GRAPHICS`, acquire DRM master, redo the modeset
//!   (the kernel console took over the CRTC in the meantime).
//!
//! `KD_TEXT` is only restored when ndlm is done with the VT, which is also when
//! the kernel console gets to use it again.

use std::fs::File;
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

// VT ioctls from <linux/vt.h> and <linux/kd.h>. They are `_IO` requests without
// an encoded argument size, hence the plain numbers.
const VT_SETMODE: libc::c_ulong = 0x5602;
const VT_GETSTATE: libc::c_ulong = 0x5603;
const VT_RELDISP: libc::c_ulong = 0x5605;
const KDSETMODE: libc::c_ulong = 0x4B3A;

const VT_AUTO: u8 = 0x00;
const VT_PROCESS: u8 = 0x01;
/// `VT_RELDISP` argument acknowledging a completed switch *to* this VT.
const VT_ACKACQ: libc::c_ulong = 0x02;
/// `VT_RELDISP` argument allowing a pending switch *away from* this VT.
const VT_RELEASE: libc::c_ulong = 0x01;

const KD_TEXT: libc::c_ulong = 0x00;
const KD_GRAPHICS: libc::c_ulong = 0x01;

const TTY_PREFIX: &str = "/dev/tty";

#[repr(C)]
struct VtStat {
    active: u16,
    signal: u16,
    state: u16,
}

#[repr(C)]
struct VtMode {
    mode: u8,
    waitv: u8,
    relsig: i16,
    acqsig: i16,
    frsig: i16,
}

/// Write end of the self-pipe the signal handler pokes to wake up the event
/// loop, `-1` until [`signal_waker`] has been called.
static WAKE_WRITE_FD: AtomicI32 = AtomicI32::new(-1);
static RELEASE_REQUESTED: AtomicBool = AtomicBool::new(false);
static ACQUIRE_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Signal handler for the VT switch handshake. Only touches atomics and writes
/// a byte to the self-pipe, both of which are async-signal-safe.
extern "C" fn on_vt_signal(signal: libc::c_int) {
    match signal {
        libc::SIGUSR1 => RELEASE_REQUESTED.store(true, Ordering::SeqCst),
        libc::SIGUSR2 => ACQUIRE_REQUESTED.store(true, Ordering::SeqCst),
        _ => return,
    }

    let fd = WAKE_WRITE_FD.load(Ordering::Relaxed);
    if fd >= 0 {
        let byte = 0u8;
        // The pipe is non-blocking, so a full pipe is not a problem: the
        // pending flags above are what the event loop acts on.
        unsafe { libc::write(fd, &byte as *const u8 as *const libc::c_void, 1) };
    }
}

/// Installs the SIGUSR1/SIGUSR2 handlers and returns the read end of the pipe
/// that becomes readable whenever the kernel wants to switch VTs.
///
/// Must be called before [`Vt::take_process_control`]: leaving a VT in
/// `VT_PROCESS` mode without a handler for the release signal kills the process
/// on the next VT switch.
pub fn signal_waker() -> io::Result<File> {
    let mut fds = [0 as RawFd; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) } != 0 {
        return Err(io::Error::last_os_error());
    }
    WAKE_WRITE_FD.store(fds[1], Ordering::SeqCst);

    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    action.sa_sigaction = on_vt_signal as *const () as usize;
    // Deliberately no SA_RESTART, so that a VT switch interrupts blocking reads.
    action.sa_flags = 0;
    unsafe { libc::sigemptyset(&mut action.sa_mask) };
    for signal in [libc::SIGUSR1, libc::SIGUSR2] {
        if unsafe { libc::sigaction(signal, &action, std::ptr::null_mut()) } != 0 {
            return Err(io::Error::last_os_error());
        }
    }

    Ok(unsafe { <File as std::os::unix::io::FromRawFd>::from_raw_fd(fds[0]) })
}

/// Drains the self-pipe once it has been reported readable.
pub fn drain_waker(waker: &File) {
    let mut buf = [0u8; 64];
    while unsafe {
        libc::read(
            waker.as_raw_fd(),
            buf.as_mut_ptr() as *mut libc::c_void,
            buf.len(),
        )
    } > 0
    {}
}

/// Returns and clears the pending VT switch requests as `(release, acquire)`.
pub fn take_switch_requests() -> (bool, bool) {
    (
        RELEASE_REQUESTED.swap(false, Ordering::SeqCst),
        ACQUIRE_REQUESTED.swap(false, Ordering::SeqCst),
    )
}

/// The virtual terminal ndlm was started on.
pub struct Vt {
    /// The terminal ndlm is connected to, as one of its standard descriptors.
    /// `None` if ndlm is not running on a virtual terminal at all, in which case
    /// there is nothing to switch between and all the operations below become
    /// no-ops.
    fd: Option<RawFd>,
    number: Option<usize>,
}

impl Vt {
    /// Determines which VT to render on.
    ///
    /// greetd hands the greeter a session whose stdin/stdout/stderr are the
    /// configured terminal, so the terminal we are connected to is the
    /// designated one.
    pub fn detect() -> Self {
        for fd in [libc::STDIN_FILENO, libc::STDOUT_FILENO, libc::STDERR_FILENO] {
            let Some(number) = ttyname(fd).and_then(|name| vt_number(&name)) else {
                continue;
            };
            return Vt {
                fd: Some(fd),
                number: Some(number),
            };
        }

        Vt {
            fd: None,
            number: None,
        }
    }

    /// Whether ndlm runs on a virtual terminal it can take over and hand back.
    pub fn is_managed(&self) -> bool {
        self.fd.is_some()
    }

    /// The VT, for log messages, e.g. `VT1`.
    pub fn name(&self) -> String {
        match self.number {
            Some(number) => format!("VT{number}"),
            None => "the current terminal".to_string(),
        }
    }

    /// Whether this VT is the one currently displayed. Without a VT to manage,
    /// the display is never taken away from us.
    pub fn is_active(&self) -> io::Result<bool> {
        let Some(fd) = self.fd else {
            return Ok(true);
        };

        let mut stat = VtStat {
            active: 0,
            signal: 0,
            state: 0,
        };
        if unsafe { libc::ioctl(fd, VT_GETSTATE, &mut stat) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Some(usize::from(stat.active)) == self.number)
    }

    fn set_mode(&self, mode: libc::c_ulong) -> io::Result<()> {
        let Some(fd) = self.fd else { return Ok(()) };
        if unsafe { libc::ioctl(fd, KDSETMODE, mode) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Tells the kernel console to keep its hands off this VT, which is the
    /// state a display owner is in.
    pub fn set_graphics_mode(&self) -> io::Result<()> {
        self.set_mode(KD_GRAPHICS)
    }

    /// Lets the kernel console use this VT again.
    pub fn set_text_mode(&self) -> io::Result<()> {
        self.set_mode(KD_TEXT)
    }

    fn set_vt_mode(&self, mode: u8) -> io::Result<()> {
        let Some(fd) = self.fd else { return Ok(()) };
        let vt_mode = VtMode {
            mode,
            waitv: 0,
            relsig: libc::SIGUSR1 as i16,
            acqsig: libc::SIGUSR2 as i16,
            frsig: 0,
        };
        if unsafe { libc::ioctl(fd, VT_SETMODE, &vt_mode) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Asks the kernel to consult us (via SIGUSR1/SIGUSR2) before and after
    /// switching away from this VT.
    pub fn take_process_control(&self) -> io::Result<()> {
        self.set_vt_mode(VT_PROCESS)
    }

    /// Gives VT switching back to the kernel, to be used when we are done.
    pub fn release_process_control(&self) -> io::Result<()> {
        self.set_vt_mode(VT_AUTO)
    }

    /// Completes a pending switch away from this VT.
    pub fn ack_release(&self) -> io::Result<()> {
        self.ack(VT_RELEASE)
    }

    /// Acknowledges that this VT has become the active one.
    pub fn ack_acquire(&self) -> io::Result<()> {
        self.ack(VT_ACKACQ)
    }

    fn ack(&self, arg: libc::c_ulong) -> io::Result<()> {
        let Some(fd) = self.fd else { return Ok(()) };
        if unsafe { libc::ioctl(fd, VT_RELDISP, arg) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

/// Returns the name of the terminal `fd` refers to.
fn ttyname(fd: RawFd) -> Option<String> {
    let mut buf = [0u8; 64];
    if unsafe {
        libc::ttyname_r(
            fd,
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len() as libc::size_t,
        )
    } != 0
    {
        return None;
    }
    let end = buf.iter().position(|b| *b == 0)?;
    String::from_utf8(buf[..end].to_vec()).ok()
}

/// Returns `N` for `/dev/ttyN`, but rejects pseudo terminals, serial consoles
/// and `/dev/tty`, none of which is a VT.
fn vt_number(name: &str) -> Option<usize> {
    let digits = name.strip_prefix(TTY_PREFIX)?;
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok().filter(|number| *number >= 1)
}
