#[must_use]
/// # Panics
/// Panics if `value` is 0
pub const fn closest_consuming_power_of_2(value: u16) -> u8 {
    assert!(value != 0, "Value must be greater than 0");

    let mut power = 0;
    let mut current = 1;

    while current < value {
        current *= 2;
        power += 1;
    }

    power
}

#[must_use]
pub const fn bits_for_length(length: u16) -> u8 {
    closest_consuming_power_of_2(length - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_128() {
        assert_eq!(bits_for_length(128), 7);
    }

    #[test]
    fn test_exact_power_of_2() {
        // Test for values that are exactly powers of 2
        assert_eq!(closest_consuming_power_of_2(2), 1);
        assert_eq!(closest_consuming_power_of_2(4), 2);
        assert_eq!(closest_consuming_power_of_2(8), 3);
        assert_eq!(closest_consuming_power_of_2(16), 4);
        assert_eq!(closest_consuming_power_of_2(32), 5);
    }

    #[test]
    fn test_non_power_of_2() {
        // Test for values that are not powers of 2, expecting the next highest power
        assert_eq!(closest_consuming_power_of_2(3), 2);
        assert_eq!(closest_consuming_power_of_2(5), 3);
        assert_eq!(closest_consuming_power_of_2(9), 4);
        assert_eq!(closest_consuming_power_of_2(17), 5);
        assert_eq!(closest_consuming_power_of_2(33), 6);
    }

    #[test]
    fn test_minimum_value() {
        // Test for the minimum input value
        assert_eq!(closest_consuming_power_of_2(1), 0);
    }

    #[test]
    #[should_panic(expected = "Value must be greater than 0")]
    const fn test_zero_input() {
        // Test for zero input, expecting a panic as the function is not designed to handle it
        let _ = closest_consuming_power_of_2(0);
    }

    #[test]
    fn test_large_value() {
        // Test for a large value to ensure algorithm efficiency and correctness
        assert_eq!(closest_consuming_power_of_2(1025), 11);
    }
}
