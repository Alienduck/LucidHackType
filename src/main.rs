use rdev::{Event, EventType, Key};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
struct GlobalState {
    is_ctrl_pressed: AtomicBool,
    is_shift_pressed: AtomicBool,
    hacker_mode_active: AtomicBool,
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

fn main() {
    let state = Arc::new(GlobalState::default());
    let state_clone = Arc::clone(&state);

    let callback = move |event: Event| -> Option<Event> {
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

                    return None;
                }
            }
            EventType::KeyRelease(key) => {
                update_modifier(&state_clone, &key, false);

                if state_clone.hacker_mode_active.load(Ordering::Relaxed) {
                    if !matches!(
                        key,
                        Key::ControlLeft
                            | Key::ControlRight
                            | Key::ShiftLeft
                            | Key::ShiftRight
                            | Key::Alt
                    ) {
                        return None;
                    }
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
}
