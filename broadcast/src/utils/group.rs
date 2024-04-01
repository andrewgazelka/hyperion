use std::borrow::Cow;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct RangeInclusive<T = u32> {
    pub start: T,
    pub end: T,
}

impl<T> RangeInclusive<T> {
    pub const fn new(start: T, end: T) -> Self {
        Self { start, end }
    }
}

struct GroupIterator<'a> {
    input: Cow<'a, [u32]>,
    current_pos: usize,
}

impl<'a> Iterator for GroupIterator<'a> {
    type Item = RangeInclusive<u32>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_pos >= self.input.len() {
            return None;
        }

        let start = self.input[self.current_pos];
        let mut end = start;

        while self.current_pos + 1 < self.input.len() && self.input[self.current_pos + 1] == end + 1
        {
            self.current_pos += 1;
            end = self.input[self.current_pos];
        }

        self.current_pos += 1; // Prepare for the next range

        Some(RangeInclusive { start, end })
    }
}

// todo: could probably make this more SIMD friendly
// todo: is Cow best practice here?
pub fn group<'a>(
    input: impl Into<Cow<'a, [u32]>>,
) -> impl Iterator<Item = RangeInclusive<u32>> + 'a {
    let input = input.into();
    debug_assert!(input.windows(2).all(|w| w[0] < w[1]));

    GroupIterator {
        input,
        current_pos: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_element_groups() {
        let input = vec![1, 3, 5];
        let result: Vec<_> = group(input).collect();
        assert_eq!(result, vec![
            RangeInclusive::new(1, 1),
            RangeInclusive::new(3, 3),
            RangeInclusive::new(5, 5),
        ]);
    }

    #[test]
    fn test_multiple_element_groups() {
        let input = vec![1, 2, 3, 6, 8];
        let result: Vec<_> = group(&input).collect();
        assert_eq!(result, vec![
            RangeInclusive::new(1, 3),
            RangeInclusive::new(6, 6),
            RangeInclusive::new(8, 8),
        ]);
    }

    #[test]
    fn test_empty_input() {
        let input: Vec<u32> = vec![];
        let result: Vec<_> = group(&input).collect();
        assert_eq!(result, Vec::<RangeInclusive<u32>>::new());
    }

    #[test]
    fn test_consecutive_and_nonconsecutive_mix() {
        let input = vec![1, 2, 3, 5, 6, 7, 9];
        let result: Vec<_> = group(&input).collect();
        assert_eq!(result, vec![
            RangeInclusive::new(1, 3),
            RangeInclusive::new(5, 7),
            RangeInclusive::new(9, 9),
        ]);
    }

    #[test]
    fn test_large_range() {
        let input = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let result: Vec<_> = group(&input).collect();
        assert_eq!(result, vec![RangeInclusive::new(1, 10),]);
    }
}
