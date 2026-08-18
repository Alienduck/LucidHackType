use clap::Parser;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use rdev::{Event, EventType, Key as RdevKey};
use std::fs;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;

#[derive(Parser)]
struct Args {
    /// Give the code file to display in Hacker Type Mode
    #[arg(short, long)]
    file: Option<String>,

    /// Give the code string to display in Hacker Type Mode
    #[arg(short, long)]
    code: Option<String>,

    /// Delay (ms) inserted between dependent synthetic input events.
    /// Purely empirical value: the target app needs time to actually process
    /// a synthetic keystroke (auto-close insertion, cursor move, etc.) before
    /// the next event is safe to send. Raise this if you still see corruption,
    /// lower it if typing feels too slow for your recording.
    #[arg(long, default_value_t = 12)]
    delay_ms: u64,

    /// Disable the "type opening delimiter then Delete the auto-inserted
    /// closing one" heuristic entirely. Turn this on (i.e. pass the flag)
    /// if you've disabled auto-closing brackets in your editor's settings,
    /// since in that case there's nothing to strip and the Delete becomes
    /// pure risk (see NOTE in strip_autoclosed_delimiter below).
    #[arg(long)]
    no_strip_brackets: bool,

    /// Disable sending Escape before every simulated Enter. This Escape is
    /// there to dismiss any open autocomplete/suggestion popup so that Enter
    /// is interpreted as "newline" and not "accept suggestion".
    #[arg(long)]
    no_dismiss_popups: bool,
}

struct State {
    active: AtomicBool,
    ctrl: AtomicBool,
    shift: AtomicBool,
    typing: AtomicBool,
    idx: AtomicUsize,
    code: Vec<char>,
    delay_ms: AtomicU64,
    strip_brackets: AtomicBool,
    dismiss_popups: AtomicBool,
}

/// Delimiters most editors auto-close by inserting the matching closer right
/// after the cursor. Deliberately EXCLUDES the single quote `'`: in Rust it's
/// ambiguous between a char literal (`'a'`, usually auto-closed) and a
/// lifetime (`'a`, which most editors are smart enough NOT to auto-close,
/// but this is editor-dependent and I haven't verified it — flagging as
/// uncertain rather than guessing). Add '\'' to this array yourself if your
/// script has no lifetimes and your editor always auto-closes quotes.
const AUTOCLOSE_TRIGGERS: [char; 4] = ['{', '[', '(', '"'];

fn sleep_ms(ms: u64) {
    if ms > 0 {
        sleep(Duration::from_millis(ms));
    }
}

/// Strips a delimiter the editor may have auto-inserted right after the
/// cursor, following a just-typed opening delimiter.
///
/// NOTE on correctness: this function cannot read the editor's actual
/// buffer content — enigo/rdev are pure input-injection APIs, there's no
/// read-back. So this is a heuristic: it assumes "I just typed an opening
/// delimiter, therefore whatever the editor auto-inserted is immediately to
/// my right, therefore Delete removes exactly that and nothing else".
///
/// If that assumption is wrong for a given editor/context (e.g. the editor
/// declined to auto-close because we're already inside an unterminated
/// string), this Delete will eat whatever character actually IS to the
/// right — and if the cursor is at true end-of-line, Delete merges the next
/// line into the current one instead, silently corrupting the output.
///
/// This is exactly why disabling auto-close brackets in the editor's own
/// settings is the more robust fix than any heuristic here: it removes the
/// need for this function entirely. Kept as an opt-out (--no-strip-brackets)
/// for that case.
fn strip_autoclosed_delimiter(enigo: &Arc<Mutex<Enigo>>, delay_ms: u64) {
    sleep_ms(delay_ms);
    if let Ok(mut e) = enigo.lock() {
        let _ = e.key(Key::Delete, Direction::Click);
    }
}

/// Clears whatever leading whitespace the editor auto-inserted after Enter,
/// then leaves the cursor at column 0 of the new (empty) line, ready for the
/// script's own indentation characters (spaces/tabs, taken verbatim from the
/// source file) to be typed normally afterwards.
///
/// Sequence: type a throwaway marker char, select back to column 0 with
/// Shift+Home (sent twice, to cover editors whose Home key is "smart" and
/// first jumps to first-non-whitespace before a second press reaches column
/// 0 — sending it twice is a no-op on editors where Home always goes
/// straight to column 0), then Backspace the whole selection including the
/// marker.
fn clear_autoindent(enigo: &Arc<Mutex<Enigo>>, delay_ms: u64) {
    if let Ok(mut e) = enigo.lock() {
        // 1. Throwaway marker: gives Shift+Home something non-empty to
        //    select back to, and anchors the selection end.
        let _ = e.text("_");
    }
    sleep_ms(delay_ms);

    if let Ok(mut e) = enigo.lock() {
        let _ = e.key(Key::Shift, Direction::Press);
    }
    sleep_ms(delay_ms);
    if let Ok(mut e) = enigo.lock() {
        let _ = e.key(Key::Home, Direction::Click);
    }
    sleep_ms(delay_ms);
    if let Ok(mut e) = enigo.lock() {
        let _ = e.key(Key::Home, Direction::Click);
    }
    sleep_ms(delay_ms);
    if let Ok(mut e) = enigo.lock() {
        let _ = e.key(Key::Shift, Direction::Release);
    }
    sleep_ms(delay_ms);
    if let Ok(mut e) = enigo.lock() {
        let _ = e.key(Key::Backspace, Direction::Click);
    }
}

fn main() {
    let args = Args::parse();

    // Strip \r (Windows line endings) so the script doesn't get confused by
    // line-ending mismatches.
    let raw_content = args.file.map_or_else(
        || args.code.expect("Requires --file or --code"),
        |f| fs::read_to_string(&f).expect("File read error"),
    );
    let content = raw_content.replace('\r', "");

    let state = Arc::new(State {
        active: AtomicBool::new(false),
        ctrl: AtomicBool::new(false),
        shift: AtomicBool::new(false),
        typing: AtomicBool::new(false),
        idx: AtomicUsize::new(0),
        code: content.chars().collect(),
        delay_ms: AtomicU64::new(args.delay_ms),
        strip_brackets: AtomicBool::new(!args.no_strip_brackets),
        dismiss_popups: AtomicBool::new(!args.no_dismiss_popups),
    });

    let enigo = Arc::new(Mutex::new(Enigo::new(&Settings::default()).unwrap()));

    let callback = move |event: Event| -> Option<Event> {
        if state.typing.load(Ordering::SeqCst) {
            return Some(event);
        }

        match event.event_type {
            EventType::KeyPress(k) => {
                match k {
                    RdevKey::ControlLeft | RdevKey::ControlRight => {
                        state.ctrl.store(true, Ordering::Relaxed)
                    }
                    RdevKey::ShiftLeft | RdevKey::ShiftRight => {
                        state.shift.store(true, Ordering::Relaxed)
                    }
                    _ => {}
                }

                if k == RdevKey::KeyH
                    && state.ctrl.load(Ordering::Relaxed)
                    && state.shift.load(Ordering::Relaxed)
                {
                    state
                        .active
                        .store(!state.active.load(Ordering::Relaxed), Ordering::Relaxed);
                    return None;
                }

                if state.active.load(Ordering::Relaxed) {
                    if k == RdevKey::Escape {
                        state.active.store(false, Ordering::Relaxed);
                        return None;
                    }
                    if matches!(
                        k,
                        RdevKey::ControlLeft
                            | RdevKey::ControlRight
                            | RdevKey::ShiftLeft
                            | RdevKey::ShiftRight
                            | RdevKey::Alt
                            | RdevKey::AltGr
                            | RdevKey::Tab
                    ) {
                        return Some(event);
                    }

                    let i = state.idx.load(Ordering::Relaxed);
                    if i < state.code.len() {
                        state.typing.store(true, Ordering::SeqCst);

                        let delay_ms = state.delay_ms.load(Ordering::Relaxed);
                        let c = state.code[i];

                        if c == '\n' {
                            if state.dismiss_popups.load(Ordering::Relaxed) {
                                if let Ok(mut e) = enigo.lock() {
                                    let _ = e.key(Key::Escape, Direction::Click);
                                }
                                sleep_ms(delay_ms);
                            }

                            if let Ok(mut e) = enigo.lock() {
                                let _ = e.text("\n");
                            }
                            sleep_ms(delay_ms);

                            clear_autoindent(&enigo, delay_ms);
                        } else {
                            if let Ok(mut e) = enigo.lock() {
                                let _ = e.text(&c.to_string());
                            }

                            if state.strip_brackets.load(Ordering::Relaxed)
                                && AUTOCLOSE_TRIGGERS.contains(&c)
                            {
                                strip_autoclosed_delimiter(&enigo, delay_ms);
                            }
                        }

                        state.idx.store(i + 1, Ordering::Relaxed);
                        state.typing.store(false, Ordering::SeqCst);
                    } else {
                        state.idx.store(0, Ordering::Relaxed);
                    }
                    return None;
                }
            }
            EventType::KeyRelease(k) => {
                match k {
                    RdevKey::ControlLeft | RdevKey::ControlRight => {
                        state.ctrl.store(false, Ordering::Relaxed)
                    }
                    RdevKey::ShiftLeft | RdevKey::ShiftRight => {
                        state.shift.store(false, Ordering::Relaxed)
                    }
                    _ => {}
                }
                if state.active.load(Ordering::Relaxed)
                    && !matches!(
                        k,
                        RdevKey::ControlLeft
                            | RdevKey::ControlRight
                            | RdevKey::ShiftLeft
                            | RdevKey::ShiftRight
                            | RdevKey::Alt
                            | RdevKey::AltGr
                    )
                {
                    return None;
                }
            }
            _ => {}
        }
        Some(event)
    };

    rdev::grab(callback).unwrap();
}
