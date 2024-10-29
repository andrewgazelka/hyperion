pub const NERD_ROCKET: char = '\u{F14DE}';
pub const FAIL_ROCKET: char = '\u{ea87}';

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_chars() {
        println!("Rocket: {NERD_ROCKET}");
        println!("Fail: {FAIL_ROCKET}");
    }
}
