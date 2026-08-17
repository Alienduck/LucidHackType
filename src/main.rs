use clap::Parser;
use enigo::{Enigo, Keyboard, Settings};
use rdev::{Event, EventType, Key};
use std::fs;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Parser)]
struct Args {
    /// Give the code file to display in Hacker Type Mode
    #[arg(short, long)]
    file: Option<String>,

    /// Give the code string to display in Hacker Type Mode
    #[arg(short, long)]
    code: Option<String>,
}

struct State {
    active: AtomicBool,
    ctrl: AtomicBool,
    shift: AtomicBool,
    typing: AtomicBool,
    idx: AtomicUsize,
    code: Vec<char>,
}

fn main() {
    let args = Args::parse();
    let content = args.file.map_or_else(
        || args.code.expect("Requires --file or --code"),
        |f| fs::read_to_string(&f).expect("File read error"),
    );

    let state = Arc::new(State {
        active: AtomicBool::new(false),
        ctrl: AtomicBool::new(false),
        shift: AtomicBool::new(false),
        typing: AtomicBool::new(false),
        idx: AtomicUsize::new(0),
        code: content.chars().collect(),
    });
    let enigo = Arc::new(Mutex::new(Enigo::new(&Settings::default()).unwrap()));

    let callback = move |event: Event| -> Option<Event> {
        if state.typing.load(Ordering::SeqCst) {
            return Some(event);
        }

        match event.event_type {
            EventType::KeyPress(k) => {
                match k {
                    Key::ControlLeft | Key::ControlRight => {
                        state.ctrl.store(true, Ordering::Relaxed)
                    }
                    Key::ShiftLeft | Key::ShiftRight => state.shift.store(true, Ordering::Relaxed),
                    _ => {}
                }
                if k == Key::KeyH
                    && state.ctrl.load(Ordering::Relaxed)
                    && state.shift.load(Ordering::Relaxed)
                {
                    let mode = !state.active.load(Ordering::Relaxed);
                    state.active.store(mode, Ordering::Relaxed);
                    return None;
                }
                if state.active.load(Ordering::Relaxed) {
                    if k == Key::Escape {
                        state.active.store(false, Ordering::Relaxed);
                        return None;
                    }
                    if matches!(
                        k,
                        Key::ControlLeft
                            | Key::ControlRight
                            | Key::ShiftLeft
                            | Key::ShiftRight
                            | Key::Alt
                            | Key::AltGr
                            | Key::Tab
                    ) {
                        return Some(event);
                    }
                    let i = state.idx.load(Ordering::Relaxed);
                    if i < state.code.len() {
                        state.idx.store(i + 1, Ordering::Relaxed);
                        state.typing.store(true, Ordering::SeqCst);
                        if let Ok(mut e) = enigo.lock() {
                            let _ = e.text(&state.code[i].to_string());
                        }
                        state.typing.store(false, Ordering::SeqCst);
                    } else {
                        state.idx.store(0, Ordering::Relaxed);
                    }
                    return None;
                }
            }
            EventType::KeyRelease(k) => {
                match k {
                    Key::ControlLeft | Key::ControlRight => {
                        state.ctrl.store(false, Ordering::Relaxed)
                    }
                    Key::ShiftLeft | Key::ShiftRight => state.shift.store(false, Ordering::Relaxed),
                    _ => {}
                }
                if state.active.load(Ordering::Relaxed)
                    && !matches!(
                        k,
                        Key::ControlLeft
                            | Key::ControlRight
                            | Key::ShiftLeft
                            | Key::ShiftRight
                            | Key::Alt
                            | Key::AltGr
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
