use super::fake::{alive, frames_script, recorded_pid, script, temp_dir};
use super::*;
use std::path::PathBuf;
use tokio::sync::mpsc::{Receiver, channel};

fn strings(args: &[OsString]) -> Vec<String> {
    args.iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect()
}

/// The position of `flag` in `args`, and the argument after it.
fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let at = args.iter().position(|a| a == flag)?;
    args.get(at + 1).map(String::as_str)
}

// ---- 3.1: argument lists and arithmetic ------------------------------------------------------

#[test]
fn every_call_is_safe_by_its_arguments() {
    let file = PathBuf::from("/cache/x y;rm -rf ~ $(id).mp4");
    for args in [
        strings(&poster_args(&file, 1)),
        strings(&playback_args(&file, 0, 480, 270)),
    ] {
        assert_eq!(args[0], "-nostdin");
        assert_eq!(flag_value(&args, "-protocol_whitelist"), Some("file"));
        for flag in ["-an", "-sn", "-dn"] {
            assert!(args.iter().any(|a| a == flag), "{flag} in {args:?}");
        }
        // The file is one argument as written, right after `-i`, after every input option.
        let i = args.iter().position(|a| a == "-i").unwrap();
        assert_eq!(args[i + 1], file.to_string_lossy());
        assert!(
            args.iter()
                .position(|a| a == "-protocol_whitelist")
                .unwrap()
                < i
        );
        assert!(
            args.iter().position(|a| a == "-ss").unwrap() < i,
            "seek before the input"
        );
        assert_eq!(
            args.last().map(String::as_str),
            Some("pipe:1"),
            "frames go to stdout"
        );
        assert_eq!(args.iter().filter(|a| a.contains("x y;rm")).count(), 1);
    }
    let probe = strings(&probe_args(&file));
    assert_eq!(flag_value(&probe, "-protocol_whitelist"), Some("file"));
    assert_eq!(
        probe.last().map(String::as_str),
        Some(&*file.to_string_lossy())
    );
}

#[test]
fn the_poster_asks_for_one_png_frame_inside_the_size_limit() {
    let args = strings(&poster_args(Path::new("/c/v.mp4"), 1));
    assert_eq!(flag_value(&args, "-ss"), Some("1"));
    assert_eq!(flag_value(&args, "-frames:v"), Some("1"));
    assert_eq!(flag_value(&args, "-c:v"), Some("png"));
    assert_eq!(flag_value(&args, "-f"), Some("image2pipe"));
    let filter = flag_value(&args, "-vf").unwrap();
    assert!(
        filter.contains("min(480,iw)") && filter.contains("min(480,ih)"),
        "{filter}"
    );
    assert!(
        filter.contains("force_original_aspect_ratio=decrease"),
        "never grows"
    );
    assert_eq!(
        flag_value(&strings(&poster_args(Path::new("/c/v.mp4"), 0)), "-ss"),
        Some("0")
    );
}

#[test]
fn playback_is_bounded_in_rate_size_and_time() {
    let at = |start| strings(&playback_args(Path::new("/c/v.mp4"), start, 480, 270));
    let args = at(0);
    assert_eq!(
        flag_value(&args, "-vf"),
        Some("fps=15,scale=480:270:flags=lanczos")
    );
    assert_eq!(flag_value(&args, "-f"), Some("rawvideo"));
    assert_eq!(flag_value(&args, "-pix_fmt"), Some("rgba"));
    assert_eq!(flag_value(&args, "-t"), Some("600"));
    assert_eq!(flag_value(&args, "-ss"), Some("0"));
    // A seek leaves less of the first ten minutes to play, and never nothing.
    assert_eq!(flag_value(&at(30), "-t"), Some("570"));
    assert_eq!(flag_value(&at(30), "-ss"), Some("30"));
    assert_eq!(flag_value(&at(599), "-t"), Some("1"));
    assert_eq!(flag_value(&at(5000), "-t"), Some("1"));
    assert_eq!(
        flag_value(
            &strings(&playback_args(Path::new("/c/v.mp4"), 0, 100, 60)),
            "-vf"
        ),
        Some("fps=15,scale=100:60:flags=lanczos")
    );
}

#[test]
fn the_duration_parser_takes_only_a_finite_positive_number() {
    assert_eq!(
        parse_duration("12.500000\n"),
        Some(Duration::from_millis(12_500))
    );
    assert_eq!(parse_duration(" 48 "), Some(Duration::from_secs(48)));
    for bad in [
        "", "N/A", "inf", "-inf", "NaN", "-3", "0", "0.0", "1e400", "abc", "1 2", "12s",
    ] {
        let got = parse_duration(bad);
        // 1e400 parses to infinity, which is refused.
        assert_eq!(got, None, "{bad:?}");
    }
    assert_eq!(
        parse_duration("1e9"),
        Some(Duration::from_secs(86_400)),
        "capped, no overflow"
    );
}

#[test]
fn fullscreen_size_fits_the_poster_in_the_target() {
    let big = (4000, 4000);
    for (poster, target, want) in [
        // The target's width is the limit for a wide picture, its height for a tall one.
        ((480, 270), (1200, 800), (1200, 674)),
        ((480, 270), (1200, 300), (532, 300)),
        ((270, 480), (1200, 800), (450, 800)),
        ((480, 480), (1000, 600), (600, 600)),
        // Never over the frame limit, whatever the target says.
        ((480, 270), big, (1280, 720)),
        ((2, 2), (100, 100), (100, 100)),
        ((480, 2), (600, 100), (600, 2)),
        ((2, 480), (100, 600), (2, 600)),
        ((0, 0), (0, 0), (2, 2)),
    ] {
        let got = fullscreen_size(poster, target);
        assert_eq!(got, want, "{poster:?} in {target:?}");
        assert!(got.0.is_multiple_of(2) && got.1.is_multiple_of(2));
        assert!(frame_len(got.0, got.1, FULLSCREEN_SIDE).is_some());
    }
}

#[test]
fn spawn_playback_refuses_a_size_over_its_limit() {
    let (tx, _rx) = channel(4);
    let mut run = run_of("ffmpeg".to_string(), 1, false);
    (run.width, run.height) = (720, 405);
    assert!(matches!(
        spawn_playback(run, tx.clone()),
        Err(VideoError::CannotShow)
    ));
    let mut run = run_of("ffmpeg".to_string(), 1, false);
    (run.width, run.height, run.max_side) = (1281, 405, FULLSCREEN_SIDE);
    assert!(matches!(
        spawn_playback(run, tx),
        Err(VideoError::CannotShow)
    ));
}

#[test]
fn frame_arithmetic() {
    assert_eq!(frame_len(480, 270, MAX_SIDE), Some(480 * 270 * 4));
    assert_eq!(frame_len(2, 2, MAX_SIDE), Some(16));
    for (w, h) in [(0, 10), (10, 0), (481, 10), (10, 481), (u32::MAX, u32::MAX)] {
        assert_eq!(frame_len(w, h, MAX_SIDE), None, "{w}x{h}");
    }
    assert_eq!(frame_len(720, 405, FULLSCREEN_SIDE), Some(720 * 405 * 4));
    assert_eq!(frame_len(1281, 10, FULLSCREEN_SIDE), None);
    assert_eq!(frame_time(0), Duration::ZERO);
    assert_eq!(frame_time(15), Duration::from_secs(1));
    assert_eq!(frame_time(1), Duration::from_micros(66_666));
    assert!(frame_time(u64::MAX).as_secs() > 0, "no overflow");
    assert_eq!(position(12, 0), Duration::from_secs(12));
    assert_eq!(position(12, 30), Duration::from_secs(14));
}

#[test]
fn a_seek_stays_inside_the_video() {
    let len = Some(Duration::from_secs(48));
    let s = Duration::from_secs;
    assert_eq!(seek_target(s(12), 5, len), 17);
    assert_eq!(seek_target(s(12), -5, len), 7);
    assert_eq!(seek_target(s(2), -5, len), 0, "back past the start");
    assert_eq!(seek_target(s(2), -30, len), 0);
    assert_eq!(
        seek_target(s(45), 5, len),
        48,
        "forward past the end goes to the end"
    );
    assert_eq!(seek_target(s(20), 30, len), 48);
    // Unknown length: the first ten minutes at most.
    assert_eq!(seek_target(s(590), 30, None), 600);
    // A video longer than ten minutes: its first ten minutes.
    assert_eq!(seek_target(s(590), 30, Some(s(1500))), 600);
    assert_eq!(seek_target(s(0), i64::MIN, len), 0);
    assert_eq!(seek_target(s(0), i64::MAX, len), 48);
}

// ---- 3.2: the runner and the poster ----------------------------------------------------------

#[test]
fn run_capped_returns_the_output_of_a_program_that_succeeds() {
    let dir = temp_dir("run-ok");
    let program = script(&dir, "ok", "printf hello");
    assert_eq!(
        run_capped(&program, &[], 100, Duration::from_secs(5)),
        Ok(b"hello".to_vec())
    );
}

#[test]
fn a_missing_program_is_reported_as_missing() {
    let dir = temp_dir("run-missing");
    let program = dir.join("no-such-ffmpeg").to_string_lossy().into_owned();
    assert_eq!(
        run_capped(&program, &[], 100, Duration::from_secs(5)),
        Err(VideoError::ProgramMissing)
    );
    assert_eq!(
        poster_png(&program, Path::new("/c/v.mp4")),
        Err(VideoError::ProgramMissing)
    );
    assert_eq!(probe_duration(&program, Path::new("/c/v.mp4")), None);
}

#[test]
fn a_program_that_fails_is_a_fixed_failure() {
    let dir = temp_dir("run-fail");
    let program = script(&dir, "bad", "printf partial; exit 3");
    assert_eq!(
        run_capped(&program, &[], 100, Duration::from_secs(5)),
        Err(VideoError::CannotShow)
    );
    // A file that is not a program at all.
    let plain = dir.join("plain");
    std::fs::write(&plain, "not a program").unwrap();
    assert_eq!(
        run_capped(&plain.to_string_lossy(), &[], 100, Duration::from_secs(5)),
        Err(VideoError::CannotShow)
    );
}

#[test]
fn a_program_that_runs_too_long_is_killed() {
    let dir = temp_dir("run-timeout");
    let program = script(
        &dir,
        "slow",
        "echo $$ > \"$(dirname \"$0\")/pid\"\nexec sleep 30",
    );
    let started = Instant::now();
    assert_eq!(
        run_capped(&program, &[], 100, Duration::from_millis(300)),
        Err(VideoError::CannotShow)
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    let pid = recorded_pid(&dir).expect("the program started");
    assert!(!alive(pid), "the program was killed and reaped");
}

#[test]
fn output_over_the_cap_is_refused_and_the_program_ended() {
    let dir = temp_dir("run-big");
    let program = script(
        &dir,
        "big",
        "echo $$ > \"$(dirname \"$0\")/pid\"\nhead -c 1000000 /dev/zero\nexec sleep 30",
    );
    assert_eq!(
        run_capped(&program, &[], 1000, Duration::from_secs(5)),
        Err(VideoError::CannotShow)
    );
    let pid = recorded_pid(&dir).expect("the program started");
    for _ in 0..100 {
        if !alive(pid) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!alive(pid), "no process remains");
}

#[test]
fn the_poster_is_taken_at_one_second_and_retried_at_the_start_for_a_short_video() {
    let dir = temp_dir("poster");
    // Writes a frame only when asked for the start: a video shorter than a second.
    let program = script(
        &dir,
        "short",
        "echo \"$*\" >> \"$(dirname \"$0\")/runs.log\"\ncase \"$*\" in *\"-ss 1 \"*) exit 0;; esac\nprintf FRAME",
    );
    assert_eq!(
        poster_png(&program, Path::new("/c/v.mp4")),
        Ok(b"FRAME".to_vec())
    );
    let runs = std::fs::read_to_string(dir.join("runs.log")).unwrap();
    let lines: Vec<&str> = runs.lines().collect();
    assert_eq!(lines.len(), 2, "{runs}");
    assert!(
        lines[0].contains("-ss 1 ") && lines[1].contains("-ss 0 "),
        "{runs}"
    );
    // A long video answers the first time and is not asked again.
    let program = script(
        &dir,
        "long",
        "echo run >> \"$(dirname \"$0\")/long.log\"\nprintf FRAME",
    );
    assert_eq!(
        poster_png(&program, Path::new("/c/v.mp4")),
        Ok(b"FRAME".to_vec())
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("long.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    // Nothing at either time is a failure.
    let program = script(&dir, "none", "exit 0");
    assert_eq!(
        poster_png(&program, Path::new("/c/v.mp4")),
        Err(VideoError::CannotShow)
    );
}

#[test]
fn the_duration_comes_from_the_probe() {
    let dir = temp_dir("probe");
    let program = script(&dir, "probe", "echo 12.5");
    assert_eq!(
        probe_duration(&program, Path::new("/c/v.mp4")),
        Some(Duration::from_millis(12_500))
    );
    let program = script(&dir, "na", "echo N/A");
    assert_eq!(probe_duration(&program, Path::new("/c/v.mp4")), None);
    let program = script(&dir, "fail", "echo 5; exit 1");
    assert_eq!(probe_duration(&program, Path::new("/c/v.mp4")), None);
}

// ---- 3.3: the playback reader ----------------------------------------------------------------

fn run_of(ffmpeg: String, generation: u64, paused: bool) -> Playback {
    Playback {
        ffmpeg,
        file: PathBuf::from("/c/v.mp4"),
        start_secs: 0,
        width: 2,
        height: 2,
        max_side: MAX_SIDE,
        generation,
        paused,
    }
}

/// The next action within `timeout`, or `None`.
fn next(rx: &mut Receiver<Action>, timeout: Duration) -> Option<Action> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(action) = rx.try_recv() {
            return Some(action);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn frames_are_paced_numbered_and_followed_by_the_end() {
    let dir = temp_dir("pace");
    let program = frames_script(&dir, "ffmpeg", 6, 16, 0);
    let (tx, mut rx) = channel(64);
    let started = Instant::now();
    let _handle = spawn_playback(run_of(program, 7, false), tx).unwrap();
    for want in 0..6u64 {
        // Handle each frame at once, as the reducer does (the reader drops frames on a backlog).
        match next(&mut rx, Duration::from_secs(5)) {
            Some(Action::VideoFrame {
                generation,
                index,
                pixels,
            }) => {
                assert_eq!((generation, index), (7, want));
                assert_eq!(pixels.len(), 16);
                assert!(
                    pixels.iter().all(|b| *b == want as u8 + 1),
                    "frame {want}: {pixels:?}"
                );
            }
            other => panic!("frame {want}: {other:?}"),
        }
    }
    assert_eq!(
        next(&mut rx, Duration::from_secs(5)),
        Some(Action::VideoEnded { generation: 7 })
    );
    // Six frames at 15 a second: the last is due 5/15 s in. Not faster.
    assert!(
        started.elapsed() >= frame_time(5),
        "{:?}",
        started.elapsed()
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn a_short_last_frame_is_dropped() {
    let dir = temp_dir("short-last");
    let program = frames_script(&dir, "ffmpeg", 3, 16, 7);
    let (tx, mut rx) = channel(64);
    let _handle = spawn_playback(run_of(program, 1, false), tx).unwrap();
    let mut frames = 0;
    loop {
        match next(&mut rx, Duration::from_secs(5)) {
            Some(Action::VideoFrame { .. }) => frames += 1,
            Some(Action::VideoEnded { generation: 1 }) => break,
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(frames, 3, "the 7 stray bytes are not a frame");
    assert_eq!(
        next(&mut rx, Duration::from_millis(100)),
        None,
        "nothing after the end"
    );
}

#[test]
fn a_program_that_writes_nothing_ends_at_once() {
    let dir = temp_dir("empty");
    let program = script(&dir, "ffmpeg", "exit 0");
    let (tx, mut rx) = channel(8);
    let _handle = spawn_playback(run_of(program, 3, false), tx).unwrap();
    assert_eq!(
        next(&mut rx, Duration::from_secs(5)),
        Some(Action::VideoEnded { generation: 3 })
    );
}

#[test]
fn a_paused_reader_sends_nothing_and_resumes_where_it_was() {
    let dir = temp_dir("pause");
    let program = frames_script(&dir, "ffmpeg", 40, 16, 0);
    let (tx, mut rx) = channel(64);
    let handle = spawn_playback(run_of(program, 1, false), tx).unwrap();
    assert!(matches!(
        next(&mut rx, Duration::from_secs(5)),
        Some(Action::VideoFrame { index: 0, .. })
    ));
    handle.set_paused(true);
    // At most one frame that was already due; then nothing for a good while.
    let mut seen = Vec::new();
    while let Some(action) = next(&mut rx, Duration::from_millis(400)) {
        seen.push(action);
    }
    assert!(seen.len() <= 2, "{seen:?}");
    let last_index = seen
        .iter()
        .filter_map(|a| match a {
            Action::VideoFrame { index, .. } => Some(*index),
            _ => None,
        })
        .next_back()
        .unwrap_or(0);
    assert!(
        next(&mut rx, Duration::from_millis(300)).is_none(),
        "paused: nothing arrives"
    );
    // Resumed: the next frame is the one after, and it comes at the pace, not in a burst.
    handle.set_paused(false);
    let resumed = Instant::now();
    let Some(Action::VideoFrame { index, .. }) = next(&mut rx, Duration::from_secs(5)) else {
        panic!("no frame after the resume");
    };
    assert_eq!(index, last_index + 1);
    let Some(Action::VideoFrame { index: after, .. }) = next(&mut rx, Duration::from_secs(5))
    else {
        panic!("no second frame");
    };
    assert_eq!(after, index + 1);
    assert!(
        resumed.elapsed() >= Duration::from_millis(50),
        "paced after a pause: {:?}",
        resumed.elapsed()
    );
}

#[test]
fn a_run_started_paused_shows_its_first_frame_and_holds() {
    let dir = temp_dir("paused-start");
    let program = frames_script(&dir, "ffmpeg", 10, 16, 0);
    let (tx, mut rx) = channel(64);
    let handle = spawn_playback(run_of(program, 4, true), tx).unwrap();
    assert!(matches!(
        next(&mut rx, Duration::from_secs(5)),
        Some(Action::VideoFrame {
            generation: 4,
            index: 0,
            ..
        })
    ));
    assert!(next(&mut rx, Duration::from_millis(400)).is_none(), "held");
    handle.set_paused(false);
    assert!(matches!(
        next(&mut rx, Duration::from_secs(5)),
        Some(Action::VideoFrame { index: 1, .. })
    ));
}

#[test]
fn a_new_run_has_its_own_generation_and_the_old_one_says_nothing_after_it_is_stopped() {
    let dir = temp_dir("generations");
    let program = frames_script(&dir, "ffmpeg", 100, 16, 0);
    let (tx, mut rx) = channel(64);
    let first = spawn_playback(run_of(program.clone(), 1, false), tx.clone()).unwrap();
    assert!(matches!(
        next(&mut rx, Duration::from_secs(5)),
        Some(Action::VideoFrame { generation: 1, .. })
    ));
    // A seek: the old run ends, a new one starts under the next number.
    drop(first);
    let _second = spawn_playback(run_of(program, 2, false), tx).unwrap();
    let mut old_after_stop = 0;
    let mut new = 0;
    let deadline = Instant::now() + Duration::from_millis(600);
    while Instant::now() < deadline {
        match next(&mut rx, Duration::from_millis(50)) {
            Some(Action::VideoFrame { generation: 1, .. }) => old_after_stop += 1,
            Some(Action::VideoFrame { generation: 2, .. }) => new += 1,
            Some(Action::VideoEnded { generation: 1 }) => panic!("a stopped run ended"),
            _ => {}
        }
    }
    assert!(new > 0, "the new run plays");
    // Frames already queued when it stopped may still arrive; the reducer drops them by number.
    assert!(old_after_stop <= 2, "{old_after_stop}");
}

#[test]
fn stopping_or_dropping_the_handle_ends_the_program() {
    let dir = temp_dir("stop");
    let program = script(
        &dir,
        "ffmpeg",
        "echo $$ > \"$(dirname \"$0\")/pid\"\nexec sleep 30",
    );
    let (tx, mut rx) = channel(8);
    let handle = spawn_playback(run_of(program, 1, false), tx).unwrap();
    let pid = recorded_pid(&dir).expect("started");
    assert!(alive(pid) || !cfg!(target_os = "linux"));
    drop(handle);
    assert!(!alive(pid), "killed and reaped when the handle is dropped");
    assert_eq!(
        next(&mut rx, Duration::from_millis(300)),
        None,
        "a stopped run does not report an end"
    );
    // `stop` does the same.
    let program = script(
        &dir,
        "ffmpeg2",
        "echo $$ > \"$(dirname \"$0\")/pid\"\nexec sleep 30",
    );
    let (tx, _rx) = channel(8);
    let _ = std::fs::remove_file(dir.join("pid"));
    let handle = spawn_playback(run_of(program, 2, false), tx).unwrap();
    let pid = recorded_pid(&dir).expect("started");
    handle.stop();
    assert!(!alive(pid));
    handle.stop();
}

#[test]
fn a_run_with_a_frame_size_over_the_limit_never_starts() {
    let dir = temp_dir("limit");
    let program = frames_script(&dir, "ffmpeg", 1, 16, 0);
    let (tx, _rx) = channel(8);
    for (w, h) in [(0, 2), (2, 0), (481, 2), (2, 481)] {
        let mut run = run_of(program.clone(), 1, false);
        run.width = w;
        run.height = h;
        assert!(spawn_playback(run, tx.clone()).is_err(), "{w}x{h}");
    }
    assert!(
        !dir.join("runs.log").exists(),
        "the program was not even started"
    );
}

#[test]
fn a_missing_program_cannot_start_a_run() {
    let dir = temp_dir("no-program");
    let (tx, _rx) = channel(8);
    let missing = dir.join("none").to_string_lossy().into_owned();
    assert_eq!(
        spawn_playback(run_of(missing, 1, false), tx).err(),
        Some(VideoError::ProgramMissing)
    );
}

#[test]
fn a_backlog_drops_frames_instead_of_piling_them_up() {
    let dir = temp_dir("backlog");
    let program = frames_script(&dir, "ffmpeg", 30, 16, 0);
    let (tx, mut rx) = channel(64);
    let _handle = spawn_playback(run_of(program, 1, false), tx).unwrap();
    // Nobody handles the frames for a while: the reader keeps at most two waiting.
    std::thread::sleep(Duration::from_millis(1500));
    let mut waiting = 0;
    let mut held = Vec::new();
    while let Ok(action) = rx.try_recv() {
        if matches!(action, Action::VideoFrame { .. }) {
            waiting += 1;
        }
        held.push(action);
    }
    assert!(waiting <= FRAMES_IN_FLIGHT, "{waiting} frames waiting");
    assert!(waiting >= 1);
}

// ---- the real programs (opt in: `cargo test -- --ignored real_ffmpeg`) -------------------------

/// Make a 3 second 1280x720 clip with the real `ffmpeg`, or `None` when it is not installed.
fn real_clip(tag: &str) -> Option<PathBuf> {
    let dir = temp_dir(tag);
    let clip = dir.join("clip.mp4");
    let made = Command::new("ffmpeg")
        .args(["-nostdin", "-loglevel", "error", "-y", "-f", "lavfi", "-i"])
        .arg("testsrc=duration=3:size=1280x720:rate=30")
        .args(["-pix_fmt", "yuv420p"])
        .arg(&clip)
        .status()
        .ok()?;
    made.success().then_some(clip)
}

#[test]
#[ignore = "needs ffmpeg and ffprobe installed"]
fn real_ffmpeg_makes_a_poster_a_length_and_paced_frames() {
    let Some(clip) = real_clip("real") else {
        eprintln!("ffmpeg is not installed: skipped");
        return;
    };
    // The poster: one PNG inside the size limit, in the video's shape.
    let png = poster_png("ffmpeg", &clip).expect("a poster");
    let poster = crate::emoji_image::decode(
        &png,
        &crate::emoji_image::attachment(crate::emoji_image::INLINE_SHRINK_TO, None),
    )
    .expect("the defensive decoder takes it");
    assert_eq!((poster.width, poster.height), (480, 270));
    assert!(frame_len(poster.width, poster.height, MAX_SIDE).is_some());
    // The length.
    let length = probe_duration("ffprobe", &clip).expect("a length");
    assert!((length.as_secs_f64() - 3.0).abs() < 0.2, "{length:?}");
    // A run from one second in: two seconds of frames at 15 a second, paced, then the end.
    let (tx, mut rx) = channel(64);
    let started = Instant::now();
    let _handle = spawn_playback(
        Playback {
            ffmpeg: "ffmpeg".to_string(),
            file: clip.clone(),
            start_secs: 1,
            width: poster.width,
            height: poster.height,
            max_side: MAX_SIDE,
            generation: 1,
            paused: false,
        },
        tx,
    )
    .expect("starts");
    let mut frames = 0;
    loop {
        match next(&mut rx, Duration::from_secs(10)) {
            Some(Action::VideoFrame { pixels, .. }) => {
                assert_eq!(pixels.len(), frame_len(480, 270, MAX_SIDE).unwrap());
                frames += 1;
            }
            Some(Action::VideoEnded { generation: 1 }) => break,
            other => panic!("{other:?}"),
        }
    }
    // The reader drops a frame the app has not handled; here every frame is taken at once.
    assert!((28..=30).contains(&frames), "{frames} frames");
    assert!(
        started.elapsed() >= Duration::from_millis(1800),
        "paced: {:?}",
        started.elapsed()
    );
    // A file that is not a video: no poster, no length, and a run that ends at once.
    let junk = clip.with_file_name("junk.mp4");
    std::fs::write(&junk, b"this is not a video").unwrap();
    assert_eq!(poster_png("ffmpeg", &junk), Err(VideoError::CannotShow));
    assert_eq!(probe_duration("ffprobe", &junk), None);
}

#[test]
fn the_play_badge_marks_the_middle_and_leaves_the_edges() {
    let poster = crate::emoji_image::Rgba {
        width: 160,
        height: 90,
        pixels: vec![200u8; 160 * 90 * 4].into(),
        ..Default::default()
    };
    let badged = with_play_badge(&poster);
    assert_eq!((badged.width, badged.height), (160, 90));
    let px = |x: usize, y: usize| badged.pixels[(y * 160 + x) * 4..][..3].to_vec();
    assert_eq!(px(0, 0), [200, 200, 200], "corners untouched");
    assert_eq!(px(159, 89), [200, 200, 200]);
    assert_eq!(px(80, 45), [255, 255, 255], "the triangle is white");
    assert_eq!(
        px(80 + 10, 45 - 10),
        [80, 80, 80],
        "the badge darkens the picture"
    );
    // The poster itself is not changed, and a tiny one gets no badge.
    assert!(poster.pixels.iter().all(|b| *b == 200));
    let tiny = crate::emoji_image::Rgba {
        width: 10,
        height: 10,
        pixels: vec![9u8; 400].into(),
        ..Default::default()
    };
    assert_eq!(with_play_badge(&tiny), tiny);
}
