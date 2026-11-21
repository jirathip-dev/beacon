use super::super::hub::Hub;
use crate::budget::over_budget;
use std::thread::{self, Scope};
use std::time::Duration;

/// The budget tests' mem, cpu and wake switches.
pub fn start<'s>(s: &'s Scope<'s, '_>, _: &'s Hub) {
    if over_budget("mem") {
        std::mem::forget(std::hint::black_box(vec![1u8; 20 << 20]));
    }
    if over_budget("cpu") {
        s.spawn(|| {
            loop {
                std::hint::black_box(0);
            }
        });
    }
    if over_budget("wake") {
        s.spawn(|| {
            loop {
                thread::sleep(Duration::from_millis(1));
            }
        });
    }
}
