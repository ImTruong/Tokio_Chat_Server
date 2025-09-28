#![allow(dead_code, unused_imports, unused_variables)]

use compact_str::CompactString;

mod characters;
mod adjectives;

use characters::CHARACTERS;
use adjectives::ADJECTIVES;

pub fn choose<T: Copy>(arrays: &[T]) -> T{
    arrays[fastrand::usize(..arrays.len())]
}

pub fn random_name() -> String {
    let adjective = choose(&ADJECTIVES);
    let character = choose(&CHARACTERS);
    format!("{} {}", adjective, character)
}

pub struct NameGenerator {
    adj_idx: usize,
    adj_offset: usize,
    char_idx: usize,
    char_offset_idx: usize,
    char_offsets: Vec<usize>,
}

impl NameGenerator {
    pub fn new() -> Self {
        let mut char_offsets: Vec<usize> = (0..CHARACTERS.len()).collect();
        fastrand::shuffle(&mut char_offsets);
        Self {
            adj_idx: 0,
            adj_offset: fastrand::usize(..ADJECTIVES.len()),
            char_idx: 0,
            char_offset_idx: 0,
            char_offsets: char_offsets,
        }
    }
    pub fn next(&mut self) -> CompactString {
        let (adj, character) = loop {
            let adj =
                ADJECTIVES[(self.adj_idx + self.adj_offset) % ADJECTIVES.len()];
            let character = CHARACTERS[(self.char_idx
                + self.char_offsets[self.char_offset_idx])
                % CHARACTERS.len()];

            self.adj_idx += 1;
            self.adj_idx %= ADJECTIVES.len();
            self.char_idx += 1;
            self.char_idx %= CHARACTERS.len();
            if self.adj_idx == 0 {
                self.char_idx = 0;
                self.char_offset_idx += 1;
                self.char_offset_idx %= self.char_offsets.len();
            }

            if (8..=18).contains(&(adj.len() + character.len())) {
                break (adj, character);
            }
        };

        let mut name = CompactString::new(adj);
        name.push_str(character);
        name
    }
}

#[macro_export]
macro_rules! b {
    ($result:expr) => {
        match $result {
            Ok(v) => v,
            Err(e) => panic!("Error: {}", e),
        }
    };
}

pub fn valid_name(name: Option<&str>) -> bool {
    match name {
        None => false,
        Some(name) => {
            if name.len() < 2 {
                return false;
            }
            if name.len() > 20 {
                return false;
            }
            name
                .chars()
                .all(|c| char::is_ascii_alphanumeric(&c) || c == '-' || c == '_')
        }
    }
}