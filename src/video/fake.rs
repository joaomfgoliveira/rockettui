//! Test support: a fake `ffmpeg` written as a shell script, so the process handling is tested
//! without the real program.

use std::path::{Path, PathBuf};

/// A fresh empty directory for one test.
pub fn temp_dir(tag: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("rockettui-test-video-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// Write `body` as an executable `sh` script called `name` in `dir`; the program to run.
pub fn script(dir: &Path, name: &str, body: &str) -> String {
    use std::os::unix::fs::PermissionsExt;
    let file = dir.join(name);
    std::fs::write(&file, format!("#!/bin/sh\n{body}\n")).expect("write script");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    file.to_string_lossy().into_owned()
}

/// A script that writes `frames` frames of `len` bytes, frame `i` filled with the byte `i + 1`,
/// then `extra` stray bytes, and exits. It records one line per run in `runs.log` beside itself.
pub fn frames_script(dir: &Path, name: &str, frames: u32, len: usize, extra: usize) -> String {
    script(
        dir,
        name,
        &format!(
            "echo \"$*\" >> \"$(dirname \"$0\")/runs.log\"\n\
             i=1\n\
             while [ $i -le {frames} ]; do\n\
               head -c {len} /dev/zero | tr '\\0' \"\\\\$(printf %o $i)\"\n\
               i=$((i+1))\n\
             done\n\
             [ {extra} -gt 0 ] && head -c {extra} /dev/zero\n\
             exit 0"
        ),
    )
}

/// Whether a process with this id still exists (Linux only; elsewhere always false).
pub fn alive(pid: u32) -> bool {
    cfg!(target_os = "linux") && Path::new(&format!("/proc/{pid}")).exists()
}

/// The process id a script recorded with `echo $$ > pid` (after `exec` it is the program's own).
pub fn recorded_pid(dir: &Path) -> Option<u32> {
    for _ in 0..200 {
        if let Ok(text) = std::fs::read_to_string(dir.join("pid"))
            && let Ok(pid) = text.trim().parse()
        {
            return Some(pid);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    None
}

/// A playback token over a fake program that only sleeps, and the program's process id: for tests
/// of what the app does with a run, and that the process is gone afterwards.
pub fn token(dir: &Path, name: &str) -> (super::PlaybackToken, u32) {
    let program = script(
        dir,
        name,
        &format!("echo $$ > \"$(dirname \"$0\")/{name}.pid\"\nexec sleep 30"),
    );
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    drop(rx);
    let handle = super::spawn_playback(
        super::Playback {
            ffmpeg: program,
            file: PathBuf::from("/c/v.mp4"),
            start_secs: 0,
            width: 2,
            height: 2,
            max_side: super::MAX_SIDE,
            generation: 1,
            paused: false,
        },
        tx,
    )
    .expect("the fake program starts");
    let pid = {
        let file = dir.join(format!("{name}.pid"));
        let mut pid = None;
        for _ in 0..200 {
            if let Ok(text) = std::fs::read_to_string(&file)
                && let Ok(n) = text.trim().parse()
            {
                pid = Some(n);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        pid.expect("the fake program recorded its pid")
    };
    (super::PlaybackToken::new(handle), pid)
}

/// Wait until the process is gone (it may take a moment to be reaped), up to a second.
pub fn gone(pid: u32) -> bool {
    for _ in 0..100 {
        if !alive(pid) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    !alive(pid)
}
