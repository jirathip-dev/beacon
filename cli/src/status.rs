use std::fmt;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Status {
    Up,
    Down,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.pad(match self {
            Status::Up => "up",
            Status::Down => "down",
        })
    }
}
