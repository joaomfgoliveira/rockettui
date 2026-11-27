//! Video through the user's own `ffmpeg` and `ffprobe`: the poster, the duration and the frames.
//!
//! The media is untrusted and the parser is large and native, so it never runs inside this
//! process. Every call is a child process started directly (an argument list, never a shell) on a
//! file of the downloads cache with a client-built name, with network protocols off, no standard
//! input, no audio, no subtitles, a fixed limit on the picture size, the number of frames and the
//! running time, and a timeout where the client waits for an answer. The child is held by a guard
//! that kills and reaps it when dropped, so nothing outlives the player or the client.
//!
//! The pure part (argument lists and arithmetic) is separate from the IO so it is unit-tested
//! without a program. The IO is tested with a fake `ffmpeg` script.
//!
//! Nothing here keeps or prints a file name, a path or the program's output: the program's stderr
//! is discarded and a failure is only one of a few fixed kinds ([`VideoError`]).

use crate::action::Action;
use std::ffi::OsString;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::Sender;

/// Frames a second the player shows at most.
pub const PLAY_FPS: u32 = 15;
/// The widest and tallest a frame is, in pixels. Every frame, and so every buffer, is at most
/// `MAX_SIDE * MAX_SIDE * 4` bytes whatever the source says.
pub const MAX_SIDE: u32 = 480;
/// The same limit for the fullscreen player: frames are at most `FULLSCREEN_SIDE` on the longer
/// side, so no buffer is ever over `FULLSCREEN_SIDE * FULLSCREEN_SIDE * 4` bytes.
pub const FULLSCREEN_SIDE: u32 = 1280;
/// The longest a video plays, in seconds: the first ten minutes.
pub const MAX_PLAY_SECS: u64 = 600;
/// How far into the video the poster is taken, in seconds.
pub const POSTER_AT_SECS: u64 = 1;
/// A short seek and a far seek, in seconds.
pub const SEEK_SECS: u64 = 5;
pub const SEEK_FAR_SECS: u64 = 30;
/// The most bytes of a poster PNG the client reads from the program.
pub const POSTER_MAX_BYTES: usize = 4 * 1024 * 1024;
/// The most bytes of `ffprobe`'s answer the client reads.
const PROBE_MAX_BYTES: usize = 4 * 1024;
/// How long the poster and the probe may take before the program is killed.
pub const POSTER_TIMEOUT: Duration = Duration::from_secs(20);
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a seek (or the start) may wait for its first frame before it is given up.
pub const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(10);
/// The longest the reader sleeps without looking at its stop and pause flags.
const SLICE: Duration = Duration::from_millis(20);

/// Why a video step failed. No payload: nothing of the file or the program's output is kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoError {
    /// The program is not installed (or not where the setting says).
    ProgramMissing,
    /// The program could not be started, failed, timed out, or gave an answer outside the limits.
    CannotShow,
    /// The video is over the download limit (announced or received).
    TooLarge,
}

// ---------------------------------------------------------------------------------------------
// Pure: argument lists and arithmetic
// ---------------------------------------------------------------------------------------------

fn os(text: &str) -> OsString {
    OsString::from(text)
}

/// The flags every `ffmpeg` call starts with: no stdin, quiet, and only plain files as input (a
/// crafted playlist cannot make the program fetch anything).
fn base_args() -> Vec<OsString> {
    ["-nostdin", "-hide_banner", "-loglevel", "error"]
        .into_iter()
        .map(os)
        .collect()
}

/// The filter that fits any picture inside `MAX_SIDE` by `MAX_SIDE` without growing it, with even
/// sides.
fn fit_filter() -> String {
    format!(
        "scale=w='min({MAX_SIDE},iw)':h='min({MAX_SIDE},ih)':force_original_aspect_ratio=decrease:force_divisible_by=2"
    )
}

/// The arguments that write one PNG frame, `at_secs` into `file`, to standard output, at most
/// `MAX_SIDE` on a side.
pub fn poster_args(file: &Path, at_secs: u64) -> Vec<OsString> {
    let mut args = base_args();
    args.extend(["-protocol_whitelist", "file", "-ss"].map(os));
    args.push(os(&at_secs.to_string()));
    args.push(os("-i"));
    args.push(file.as_os_str().to_owned());
    args.extend(["-frames:v", "1", "-an", "-sn", "-dn", "-vf"].map(os));
    args.push(os(&fit_filter()));
    args.extend(["-f", "image2pipe", "-c:v", "png", "pipe:1"].map(os));
    args
}

/// The arguments that write the raw RGBA frames of `file` from `start_secs`, `width` by `height`
/// pixels, `PLAY_FPS` a second, to standard output. At most the first `MAX_PLAY_SECS` seconds of
/// the video are played, so a seek leaves less to play.
pub fn playback_args(file: &Path, start_secs: u64, width: u32, height: u32) -> Vec<OsString> {
    let mut args = base_args();
    args.extend(["-protocol_whitelist", "file", "-ss"].map(os));
    args.push(os(&start_secs.to_string()));
    args.push(os("-i"));
    args.push(file.as_os_str().to_owned());
    args.push(os("-t"));
    args.push(os(&MAX_PLAY_SECS
        .saturating_sub(start_secs)
        .max(1)
        .to_string()));
    args.extend(["-an", "-sn", "-dn", "-vf"].map(os));
    args.push(os(&format!(
        "fps={PLAY_FPS},scale={width}:{height}:flags=lanczos"
    )));
    args.extend(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"].map(os));
    args
}

/// The arguments that make `ffprobe` print the length of `file` in seconds and nothing else.
pub fn probe_args(file: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "-v",
        "error",
        "-protocol_whitelist",
        "file",
        "-show_entries",
        "format=duration",
        "-of",
        "default=noprint_wrappers=1:nokey=1",
        "-i",
    ]
    .into_iter()
    .map(os)
    .collect();
    args.push(file.as_os_str().to_owned());
    args
}

/// The length `ffprobe` printed (`12.345000`), `None` for anything that is not a finite, positive
/// number of seconds (`N/A`, empty, negative, `inf`, text). Never longer than a day, so the
/// arithmetic downstream cannot overflow.
pub fn parse_duration(output: &str) -> Option<Duration> {
    let seconds: f64 = output.trim().parse().ok()?;
    if !seconds.is_finite() || seconds <= 0.0 {
        return None;
    }
    Some(Duration::from_secs_f64(seconds.min(86_400.0)))
}

/// The bytes of one `width` by `height` RGBA frame, `None` for an empty picture or one with a
/// side over `max_side` (`MAX_SIDE` inline, `FULLSCREEN_SIDE` fullscreen).
pub fn frame_len(width: u32, height: u32, max_side: u32) -> Option<usize> {
    if width == 0 || height == 0 || width > max_side || height > max_side {
        return None;
    }
    usize::try_from(u64::from(width) * u64::from(height) * 4).ok()
}

/// The fullscreen frame size: the poster's shape, as large as fits in `target` pixels (the
/// overlay's cells times the cell size), so the terminal shows it near one to one. Each side is
/// at most `FULLSCREEN_SIDE`, even, and at least 2. A poster smaller than that is scaled up,
/// which wastes a little work but keeps the picture right.
pub fn fullscreen_size(poster: (u32, u32), target: (u32, u32)) -> (u32, u32) {
    let limit = FULLSCREEN_SIDE;
    let (bw, bh) = (
        u64::from(target.0.clamp(2, limit)),
        u64::from(target.1.clamp(2, limit)),
    );
    let (w, h) = (u64::from(poster.0.max(1)), u64::from(poster.1.max(1)));
    let (fw, fh) = if w * bh >= h * bw {
        (bw, h * bw / w)
    } else {
        (w * bh / h, bh)
    };
    let even = |n: u64, max: u64| (n & !1).clamp(2, max & !1) as u32;
    (even(fw, bw), even(fh, bh))
}

/// When frame number `index` is due, counted from the start of the run.
pub fn frame_time(index: u64) -> Duration {
    Duration::from_micros(index.saturating_mul(1_000_000) / u64::from(PLAY_FPS))
}

/// The position of frame `index` of a run that started at `start_secs`.
pub fn position(start_secs: u64, index: u64) -> Duration {
    Duration::from_secs(start_secs) + frame_time(index)
}

/// Where a seek of `delta` seconds (negative is back) from `position` lands: the start at the
/// earliest, and the end of the video at the latest (or of the first `MAX_PLAY_SECS` when the
/// length is unknown or longer). Whole seconds.
pub fn seek_target(position: Duration, delta_secs: i64, duration: Option<Duration>) -> u64 {
    let end = duration.map_or(MAX_PLAY_SECS, |d| d.as_secs().min(MAX_PLAY_SECS));
    let from = i64::try_from(position.as_secs()).unwrap_or(i64::MAX);
    let to = from.saturating_add(delta_secs).max(0);
    u64::try_from(to).unwrap_or(0).min(end)
}

/// `poster` with a play button drawn in its middle: a dark round badge with a white triangle, so a
/// still frame reads as a video. Drawn on a copy for the screen only; the cached poster is
/// untouched, and playing frames replace it. A picture too small for a legible badge is returned
/// as it is.
pub fn with_play_badge(poster: &crate::emoji_image::Rgba) -> crate::emoji_image::Rgba {
    let (w, h) = (poster.width as i64, poster.height as i64);
    let radius = w.min(h) / 5;
    if radius < 6 || poster.pixels.len() != (w * h * 4) as usize {
        return poster.clone();
    }
    let mut pixels = poster.pixels.to_vec();
    let (cx, cy) = (w / 2, h / 2);
    // The triangle points right, centred a little left so it looks centred in the circle.
    let (left, right) = (cx - radius / 3, cx + radius / 2);
    let half_height = radius / 2;
    for y in (cy - radius)..=(cy + radius) {
        for x in (cx - radius)..=(cx + radius) {
            if x < 0 || y < 0 || x >= w || y >= h {
                continue;
            }
            let (dx, dy) = (x - cx, y - cy);
            if dx * dx + dy * dy > radius * radius {
                continue;
            }
            let at = ((y * w + x) * 4) as usize;
            // Darken what is under the badge (about 60%).
            for c in 0..3 {
                pixels[at + c] = (u16::from(pixels[at + c]) * 2 / 5) as u8;
            }
            // Inside the triangle: from `left` to the point at `right`, narrowing to it.
            let across = (right - left).max(1);
            let reach = right - x;
            if x >= left && x <= right && (y - cy).abs() * across <= half_height * reach {
                pixels[at..at + 3].copy_from_slice(&[255, 255, 255]);
            }
        }
    }
    crate::emoji_image::Rgba {
        pixels: pixels.into(),
        ..poster.clone()
    }
}

// ---------------------------------------------------------------------------------------------
// The child process guard and the bounded runner
// ---------------------------------------------------------------------------------------------

/// A child process that is killed and reaped when the guard is dropped (or by [`kill`]), so a
/// replaced player, a stopped one, an error and the client exiting all end it. Shared so the owner
/// of a player can kill the child while a thread is blocked reading its output.
///
/// [`kill`]: ChildGuard::kill
#[derive(Clone)]
pub struct ChildGuard(Arc<Mutex<Option<Child>>>);

impl ChildGuard {
    fn new(child: Child) -> ChildGuard {
        ChildGuard(Arc::new(Mutex::new(Some(child))))
    }

    /// Kill the child and reap it. Safe to call more than once.
    pub fn kill(&self) {
        let child = self.0.lock().ok().and_then(|mut slot| slot.take());
        if let Some(mut child) = child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// The exit status if the child has ended; `None` while it runs.
    fn try_wait(&self) -> Option<std::process::ExitStatus> {
        self.0.lock().ok().and_then(|mut slot| {
            slot.as_mut()
                .and_then(|child| child.try_wait().ok().flatten())
        })
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        // The last owner ends the child; a clone held elsewhere keeps it alive.
        if Arc::strong_count(&self.0) == 1 {
            self.kill();
        }
    }
}

/// `spawn`, trying again a few times when the program file is briefly busy (it was only just
/// written, as with a script that was installed a moment ago).
fn spawn_with_retry(program: &str, args: &[OsString]) -> std::io::Result<Child> {
    let mut tries = 0;
    loop {
        let spawned = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        match spawned {
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy && tries < 20 => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(10));
            }
            other => return other,
        }
    }
}

/// Start `program` with `args`, standard input and error discarded, standard output piped. `Err`
/// says whether the program is missing or just could not start.
fn start(
    program: &str,
    args: &[OsString],
) -> Result<(ChildGuard, std::process::ChildStdout), VideoError> {
    let mut child = spawn_with_retry(program, args).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => VideoError::ProgramMissing,
        _ => VideoError::CannotShow,
    })?;
    let stdout = child.stdout.take().ok_or(VideoError::CannotShow);
    let guard = ChildGuard::new(child);
    // On a missing pipe the guard drops here and the child is killed.
    Ok((guard, stdout?))
}

/// Run `program` to the end and return what it wrote to standard output, at most `cap` bytes. A
/// program that is still running at `timeout`, writes more than `cap`, or exits with an error is
/// killed and reported as [`VideoError::CannotShow`]; none of its output is kept in that case.
pub fn run_capped(
    program: &str,
    args: &[OsString],
    cap: usize,
    timeout: Duration,
) -> Result<Vec<u8>, VideoError> {
    let (guard, mut stdout) = start(program, args)?;
    let deadline = Instant::now() + timeout;
    let (tx, rx) = std::sync::mpsc::channel();
    // A separate thread reads, so this one can enforce the timeout while the pipe is quiet. It
    // ends when the program is killed (EOF) or the cap is passed.
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut buf = [0u8; 16 * 1024];
        let result = loop {
            match stdout.read(&mut buf) {
                Ok(0) => break Ok(bytes),
                Ok(n) => {
                    if bytes.len() + n > cap {
                        break Err(VideoError::CannotShow);
                    }
                    bytes.extend_from_slice(&buf[..n]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => break Err(VideoError::CannotShow),
            }
        };
        let _ = tx.send(result);
    });
    let remaining = deadline.saturating_duration_since(Instant::now());
    let bytes = match rx.recv_timeout(remaining) {
        Ok(Ok(bytes)) => bytes,
        // Too much, a read error, or the timeout: end the program.
        Ok(Err(e)) => {
            guard.kill();
            return Err(e);
        }
        Err(_) => {
            guard.kill();
            return Err(VideoError::CannotShow);
        }
    };
    // The pipe closed; the program should be exiting. Give it until the deadline.
    loop {
        match guard.try_wait() {
            Some(status) if status.success() => return Ok(bytes),
            Some(_) => return Err(VideoError::CannotShow),
            None if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            None => {
                guard.kill();
                return Err(VideoError::CannotShow);
            }
        }
    }
}

/// One PNG frame of `file`, about a second in (the first frame when the video is shorter). The
/// bytes are the program's and are not trusted: the caller decodes them with the defensive decoder.
pub fn poster_png(ffmpeg: &str, file: &Path) -> Result<Vec<u8>, VideoError> {
    let at = |secs| {
        run_capped(
            ffmpeg,
            &poster_args(file, secs),
            POSTER_MAX_BYTES,
            POSTER_TIMEOUT,
        )
    };
    match at(POSTER_AT_SECS) {
        // A video shorter than that writes no frame at all (and exits cleanly), or fails to seek.
        Ok(bytes) if !bytes.is_empty() => Ok(bytes),
        Err(VideoError::ProgramMissing) => Err(VideoError::ProgramMissing),
        _ => match at(0)? {
            bytes if bytes.is_empty() => Err(VideoError::CannotShow),
            bytes => Ok(bytes),
        },
    }
}

/// The length of `file` from `ffprobe`; `None` when it cannot be read, in which case the player
/// shows no length and plays until the end of the stream.
pub fn probe_duration(ffprobe: &str, file: &Path) -> Option<Duration> {
    let bytes = run_capped(ffprobe, &probe_args(file), PROBE_MAX_BYTES, PROBE_TIMEOUT).ok()?;
    parse_duration(std::str::from_utf8(&bytes).ok()?)
}

// ---------------------------------------------------------------------------------------------
// The playback reader
// ---------------------------------------------------------------------------------------------

/// What the reader thread is told: stop, or pause (it then stops reading, so the program blocks
/// on its full pipe and needs no signal).
#[derive(Default)]
struct Control {
    stop: AtomicBool,
    paused: Mutex<bool>,
    changed: Condvar,
}

impl Control {
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    fn is_paused(&self) -> bool {
        self.paused.lock().is_ok_and(|p| *p)
    }

    /// Wait while paused, returning true if it did wait. Returns at once on stop.
    fn wait_while_paused(&self) -> bool {
        let Ok(mut paused) = self.paused.lock() else {
            return false;
        };
        let mut waited = false;
        while *paused && !self.stopped() {
            waited = true;
            paused = match self.changed.wait_timeout(paused, SLICE) {
                Ok((guard, _)) => guard,
                Err(_) => return waited,
            };
        }
        waited
    }
}

/// What starts one playback run.
pub struct Playback {
    pub ffmpeg: String,
    pub file: std::path::PathBuf,
    /// Seconds into the video the run starts at.
    pub start_secs: u64,
    pub width: u32,
    pub height: u32,
    /// The most a side of the frame may be (`MAX_SIDE` or `FULLSCREEN_SIDE`); a larger size is
    /// refused before anything starts.
    pub max_side: u32,
    /// The run's number: every frame it sends carries it, so frames of an older run are ignored.
    pub generation: u64,
    /// Start paused: the first frame is sent, then the reader waits for a resume.
    pub paused: bool,
}

/// One running playback: the program and the thread that reads and paces its frames. Dropping it
/// (or [`stop`]) kills and reaps the program and tells the thread to end; nothing waits on the
/// thread, which finishes by itself within a moment.
///
/// [`stop`]: PlaybackHandle::stop
pub struct PlaybackHandle {
    guard: ChildGuard,
    control: Arc<Control>,
}

impl PlaybackHandle {
    /// Pause or resume. A paused reader holds its place and sends nothing.
    pub fn set_paused(&self, paused: bool) {
        if let Ok(mut flag) = self.control.paused.lock() {
            *flag = paused;
        }
        self.control.changed.notify_all();
    }

    /// End the run now.
    pub fn stop(&self) {
        self.control.stop.store(true, Ordering::SeqCst);
        self.control.changed.notify_all();
        self.guard.kill();
    }
}

impl Drop for PlaybackHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// A running playback that an action can carry: the app keeps one clone and ends the run through
/// it, so the process outlives neither the player nor the client. Equal when it is the same run.
#[derive(Clone)]
pub struct PlaybackToken(Arc<PlaybackHandle>);

impl PlaybackToken {
    pub fn new(handle: PlaybackHandle) -> PlaybackToken {
        PlaybackToken(Arc::new(handle))
    }

    /// Pause or resume the run.
    pub fn set_paused(&self, paused: bool) {
        self.0.set_paused(paused);
    }

    /// End the run now (the process is killed and reaped).
    pub fn stop(&self) {
        self.0.stop();
    }
}

impl PartialEq for PlaybackToken {
    fn eq(&self, other: &PlaybackToken) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl std::fmt::Debug for PlaybackToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PlaybackToken")
    }
}

/// How many frames may be waiting in the app's channel before the reader drops a frame instead of
/// sending one. The reducer keeps only the newest, so a backlog is only memory.
const FRAMES_IN_FLIGHT: usize = 2;

/// Start `run`: the program, and a thread that reads whole frames from its pipe, shows each at
/// its time (`start + n / PLAY_FPS`, the thread owns the pace) by sending `Action::VideoFrame`,
/// and sends `Action::VideoEnded` when the stream ends. A frame that arrives while earlier ones
/// are still unhandled is dropped. A stream that ends in the middle of a frame drops that frame.
pub fn spawn_playback(run: Playback, tx: Sender<Action>) -> Result<PlaybackHandle, VideoError> {
    let len = frame_len(run.width, run.height, run.max_side).ok_or(VideoError::CannotShow)?;
    let args = playback_args(&run.file, run.start_secs, run.width, run.height);
    let (guard, stdout) = start(&run.ffmpeg, &args)?;
    let control = Arc::new(Control {
        paused: Mutex::new(run.paused),
        ..Control::default()
    });
    let handle = PlaybackHandle {
        guard,
        control: control.clone(),
    };
    let generation = run.generation;
    std::thread::spawn(move || read_frames(stdout, len, generation, &control, &tx));
    Ok(handle)
}

/// Send `action` without ever blocking the thread for long: it gives up when the run is stopped.
/// False when the run is over (stopped, or the receiver is gone).
fn send_unless_stopped(tx: &Sender<Action>, control: &Control, mut action: Action) -> bool {
    use tokio::sync::mpsc::error::TrySendError;
    loop {
        if control.stopped() {
            return false;
        }
        match tx.try_send(action) {
            Ok(()) => return true,
            Err(TrySendError::Closed(_)) => return false,
            Err(TrySendError::Full(back)) => {
                action = back;
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

/// The instant `ago` before now, or now when the clock cannot go back that far.
fn back_from_now(ago: Duration) -> Instant {
    let now = Instant::now();
    now.checked_sub(ago).unwrap_or(now)
}

fn read_frames(
    mut stdout: impl Read,
    len: usize,
    generation: u64,
    control: &Control,
    tx: &Sender<Action>,
) {
    let mut start = Instant::now();
    let mut index: u64 = 0;
    let mut in_flight: Vec<Weak<[u8]>> = Vec::new();
    loop {
        if control.stopped() {
            return;
        }
        // A paused run reads nothing, except that a run that starts paused shows its first frame.
        if index > 0 && control.wait_while_paused() {
            // Resuming: the clock continues from this frame, with no burst to catch up.
            start = back_from_now(frame_time(index));
        }
        if control.stopped() {
            return;
        }
        let mut pixels = vec![0u8; len];
        if stdout.read_exact(&mut pixels).is_err() {
            // The end of the stream, a short last frame, or the program was killed.
            if !control.stopped() {
                send_unless_stopped(tx, control, Action::VideoEnded { generation });
            }
            return;
        }
        // Wait for the frame's time in slices, so a stop or a pause is seen quickly.
        let due = start + frame_time(index);
        let mut held = false;
        loop {
            if control.stopped() {
                return;
            }
            if index > 0 && control.is_paused() {
                // Paused while waiting: hold this frame; the clock continues on resume.
                control.wait_while_paused();
                held = true;
                continue;
            }
            let now = Instant::now();
            if held {
                start = back_from_now(frame_time(index));
                break;
            }
            if now >= due {
                break;
            }
            std::thread::sleep((due - now).min(SLICE));
        }
        in_flight.retain(|frame| frame.strong_count() > 0);
        if in_flight.len() < FRAMES_IN_FLIGHT {
            let pixels: Arc<[u8]> = pixels.into();
            in_flight.push(Arc::downgrade(&pixels));
            let frame = Action::VideoFrame {
                generation,
                index,
                pixels,
            };
            if !send_unless_stopped(tx, control, frame) {
                return;
            }
        }
        index += 1;
    }
}

#[cfg(test)]
pub(crate) mod fake;
#[cfg(test)]
mod tests;
