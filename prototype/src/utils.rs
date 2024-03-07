struct SplitIntoMut<'a, T> {
    input: Option<&'a mut [T]>,
    count: usize,
    lower_count: usize,
    num_chunks_with_rem: usize,
}

impl<'a, T> SplitIntoMut<'a, T> {
    fn new(count: usize, input: &'a mut [T]) -> Self {
        let input_len = input.len();

        // calculate div and remainder
        #[allow(clippy::integer_division)]
        let lower_count = input_len / count;
        let num_chunks_with_rem = input_len % count;

        Self {
            input: Some(input),
            count,
            lower_count,
            num_chunks_with_rem,
        }
    }
}

impl<'a, T> Iterator for SplitIntoMut<'a, T> {
    type Item = &'a mut [T];

    fn next(&mut self) -> Option<Self::Item> {
        let input = self.input.take()?;

        let amount_to_take = if self.num_chunks_with_rem > 0 {
            self.num_chunks_with_rem -= 1;
            self.lower_count + 1
        } else {
            self.lower_count
        };

        let (split, rest) = input.split_at_mut(amount_to_take);

        if !rest.is_empty() {
            self.input = Some(rest);
        }

        Some(split)
    }
}

pub fn split_into_mut<T>(count: usize, input: &mut [T]) -> impl Iterator<Item = &mut [T]> {
    SplitIntoMut::new(count, input)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_split_into_mut() {
        let mut input = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let mut iter = super::split_into_mut(3, &mut input);

        assert_eq!(iter.next().unwrap(), &[1, 2, 3, 4]);
        assert_eq!(iter.next().unwrap(), &[5, 6, 7]);
        assert_eq!(iter.next().unwrap(), &[8, 9, 10]);
        assert!(iter.next().is_none());
        
        let mut input = [1, 2, 3, 4, 5, 6, 7, 8, 9];
        let mut iter = super::split_into_mut(3, &mut input);
        
        assert_eq!(iter.next().unwrap(), &[1, 2, 3]);
        assert_eq!(iter.next().unwrap(), &[4, 5, 6]);
        assert_eq!(iter.next().unwrap(), &[7, 8, 9]);
        
        let mut input = [1, 2, 3, 4, 5, 6, 7, 8, 9];
        let mut iter = super::split_into_mut(4, &mut input);
        
        assert_eq!(iter.next().unwrap(), &[1, 2, 3]);
        assert_eq!(iter.next().unwrap(), &[4, 5]);
        assert_eq!(iter.next().unwrap(), &[6, 7]);
        assert_eq!(iter.next().unwrap(), &[8, 9]);
    }
}
