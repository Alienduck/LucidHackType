use clap::Parser;
use enigo::{Enigo, Keyboard, Settings};
use rdev::{Event, EventType, Key};
use std::fs;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Parser)]
struct Args {
    /// Give the code file to display in Hacker Type Mode
    #[arg(short, long)]
    file: Option<String>,

    /// Give the code string to display in Hacker Type Mode
    #[arg(short, long)]
    code: Option<String>,
}

struct GlobalState {
    is_ctrl_pressed: AtomicBool,
    is_shift_pressed: AtomicBool,
    hacker_mode_active: AtomicBool,
    target_code: Vec<char>,
    current_index: AtomicUsize,
    is_enigo_typing: AtomicBool,
    last_physical_keypress: Mutex<Instant>,
}

impl GlobalState {
    fn new(code_str: String) -> Self {
        Self {
            is_ctrl_pressed: AtomicBool::new(false),
            is_shift_pressed: AtomicBool::new(false),
            hacker_mode_active: AtomicBool::new(false),
            target_code: code_str.chars().collect(),
            current_index: AtomicUsize::new(0),
            is_enigo_typing: AtomicBool::new(false),
            last_physical_keypress: Mutex::new(Instant::now()),
        }
    }
}

fn update_modifier(state: &GlobalState, key: &Key, is_pressed: bool) {
    match key {
        Key::ControlLeft | Key::ControlRight => {
            state.is_ctrl_pressed.store(is_pressed, Ordering::Relaxed);
        }
        Key::ShiftLeft | Key::ShiftRight => {
            state.is_shift_pressed.store(is_pressed, Ordering::Relaxed);
        }
        _ => {}
    }
}

fn main() -> Result<(), std::io::Error> {
    let args = Args::parse();

    let content = if let Some(file_path) = args.file {
        fs::read_to_string(&file_path)
            .unwrap_or_else(|_| panic!("Impossible de lire le fichier : {}", file_path))
    } else if let Some(code_str) = args.code {
        code_str
    } else {
        panic!("Erreur : Vous devez fournir soit un fichier (--file) soit une chaîne (--code).");
    };

    let state = Arc::new(GlobalState::new(content));
    let state_clone = Arc::clone(&state);

    let (tx, rx) = mpsc::channel::<char>();
    let state_for_thread = Arc::clone(&state);

    thread::spawn(move || {
        let mut enigo = Enigo::new(&Settings::default()).expect("Failed to create Enigo");

        while let Ok(c) = rx.recv() {
            let mut s = String::new();
            s.push(c);

            state_for_thread
                .is_enigo_typing
                .store(true, Ordering::SeqCst);

            let _ = enigo.text(&s);

            thread::sleep(Duration::from_millis(0));

            state_for_thread
                .is_enigo_typing
                .store(false, Ordering::SeqCst);
        }
    });

    let callback = move |event: Event| -> Option<Event> {
        if state_clone.is_enigo_typing.load(Ordering::SeqCst) {
            return Some(event);
        }

        match event.event_type {
            EventType::KeyPress(key) => {
                update_modifier(&state_clone, &key, true);

                if key == Key::KeyH
                    && state_clone.is_ctrl_pressed.load(Ordering::Relaxed)
                    && state_clone.is_shift_pressed.load(Ordering::Relaxed)
                {
                    let current_mode = state_clone.hacker_mode_active.load(Ordering::Relaxed);
                    state_clone
                        .hacker_mode_active
                        .store(!current_mode, Ordering::Relaxed);
                    println!("Hacker mode enable : {}", !current_mode);
                    return None;
                }

                if state_clone.hacker_mode_active.load(Ordering::Relaxed) {
                    if key == Key::Escape {
                        state_clone
                            .hacker_mode_active
                            .store(false, Ordering::Relaxed);
                        println!("Hacker mode disable (Escape)");
                        return None;
                    }

                    if let Ok(mut last_press) = state_clone.last_physical_keypress.lock() {
                        if last_press.elapsed() < Duration::from_millis(15) {
                            return None;
                        }
                        *last_press = Instant::now();
                    }

                    if matches!(
                        key,
                        Key::ControlLeft
                            | Key::ControlRight
                            | Key::ShiftLeft
                            | Key::ShiftRight
                            | Key::Alt
                            | Key::ScrollLock
                    ) {
                        return Some(event);
                    }

                    let idx = state_clone.current_index.load(Ordering::Relaxed);

                    if idx < state_clone.target_code.len() {
                        let next_char = state_clone.target_code[idx];
                        state_clone.current_index.store(idx + 1, Ordering::Relaxed);

                        let _ = tx.send(next_char);

                        return None;
                    } else {
                        return None;
                    }
                }
            }
            EventType::KeyRelease(key) => {
                update_modifier(&state_clone, &key, false);

                if state_clone.hacker_mode_active.load(Ordering::Relaxed) {
                    if matches!(
                        key,
                        Key::ControlLeft
                            | Key::ControlRight
                            | Key::ShiftLeft
                            | Key::ShiftRight
                            | Key::Alt
                    ) {
                        return Some(event);
                    }
                    return None;
                }
            }
            _ => {}
        };

        Some(event)
    };

    println!("Starting... Press Ctrl+Shift+H to enable the Hacker Typer.");

    if let Err(error) = rdev::grab(callback) {
        println!("Error while attempt to grab : {:?}", error);
    }
    Ok(())
}
