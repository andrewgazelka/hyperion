pub const NERD_ROCKET: char = '\u{F14DE}';

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_chars() {
        println!("Rocket: {NERD_ROCKET}");
    }
}
