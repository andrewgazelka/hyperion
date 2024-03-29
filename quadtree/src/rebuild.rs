use std::{cmp::Reverse, iter::Peekable, ops::AddAssign};

use itertools::Itertools;
use num_traits::PrimInt;

#[derive(Copy, Clone)]
pub struct MoveElement {
    pub remove_from_idx: usize,
    pub insert_to_idx: usize,
}

struct OrderedEvents {
    by_removal: Vec<MoveElement>,
    by_insertion: Vec<MoveElement>,
}

impl From<Vec<MoveElement>> for OrderedEvents {
    fn from(changes: Vec<MoveElement>) -> Self {
        let mut by_removal = changes.clone();
        by_removal.sort_by_key(|x| Reverse(x.remove_from_idx));

        let mut by_insertion = changes;
        by_insertion.sort_by_key(|x| Reverse(x.insert_to_idx));

        Self {
            by_removal,
            by_insertion,
        }
    }
}

enum Event {
    Removal(usize),
    Insert { from: usize, to: usize },
}

impl Iterator for OrderedEvents {
    type Item = Event;

    fn next(&mut self) -> Option<Self::Item> {
        let soonest_removal = self.by_removal.last().copied();
        let soonest_insertion = self.by_insertion.last().copied();

        match (soonest_removal, soonest_insertion) {
            (Some(removal), None) => {
                self.by_removal.pop();
                Some(Event::Removal(removal.remove_from_idx))
            }
            (None, Some(insertion)) => {
                self.by_insertion.pop();
                let from = insertion.remove_from_idx;
                let to = insertion.insert_to_idx;
                Some(Event::Insert { from, to })
            }
            (Some(removal), Some(insertion)) => {
                if removal.remove_from_idx <= insertion.insert_to_idx {
                    self.by_removal.pop();
                    Some(Event::Removal(removal.remove_from_idx))
                } else {
                    self.by_insertion.pop();
                    let from = insertion.remove_from_idx;
                    let to = insertion.insert_to_idx;
                    Some(Event::Insert { from, to })
                }
            }
            (None, None) => None,
        }
    }
}

struct TrackingUpdate<I> {
    tracking: I,
}

impl<'a, T, I> TrackingUpdate<Peekable<I>>
where
    T: PrimInt + AddAssign + 'a,
    I: Iterator<Item = &'a mut T> + 'a,
{
    fn update(&mut self, until_idx: usize, offset: isize) {
        while let Some(idx) = self.tracking.peek() {
            if idx.to_usize().unwrap() > until_idx {
                break;
            }

            let offset = T::from(offset).unwrap();

            *self.tracking.next().unwrap() += offset;
        }
    }

    fn new(tracking: I) -> Self
where {
        Self {
            tracking: tracking.peekable(),
        }
    }
}

fn debug_assert_valid_changes(changes: &[MoveElement]) {
    debug_assert!(
        changes.iter().map(|x| x.remove_from_idx).all_unique(),
        "removal indices must be unique"
    );
    debug_assert!(
        changes.iter().all(|x| x.remove_from_idx != x.insert_to_idx),
        "removal and insertion indices must be different"
    );
}

#[allow(clippy::indexing_slicing)]
#[allow(dead_code)]
pub fn apply_vec<T, Idx>(
    input: &[T],
    changes: &[MoveElement],
    tracking: &mut [Idx], // 3 9 10
) -> Vec<T>
where
    T: Copy,
    Idx: PrimInt + AddAssign,
{
    debug_assert_valid_changes(changes);

    let len = input.len();
    let mut result = Vec::with_capacity(len);
    let ordered_events = OrderedEvents::from(changes.to_vec());

    let tracking = tracking.iter_mut();
    let mut tracking = TrackingUpdate::new(tracking);

    let mut src_idx = 0;
    let mut offset = 0isize;

    for event in ordered_events {
        match event {
            Event::Removal(removal) => {
                debug_assert!(removal < len, "attempt to move element from invalid index");

                result.extend_from_slice(&input[src_idx..removal]);
                src_idx = removal + 1;

                tracking.update(removal, offset);

                offset -= 1;
            }
            Event::Insert { from, to } => {
                debug_assert!(from < len, "attempt to move element from invalid index");
                debug_assert!(to < len, "attempt to move element to invalid index");

                result.extend_from_slice(&input[src_idx..=to]);
                let elem = input[from];
                result.push(elem);
                src_idx = to + 1;

                tracking.update(to, offset);

                offset += 1;
            }
        }
    }

    result.extend_from_slice(&input[src_idx..]);

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_changes() {
        let input = vec![1, 2, 3, 4, 5];
        let changes = vec![];
        let mut tracking = vec![0];

        let expected = vec![1, 2, 3, 4, 5];

        assert_eq!(apply_vec(&input, &changes, &mut tracking), expected);
        assert_eq!(tracking, vec![0]);
    }

    #[test]
    fn test_single_move() {
        //                          0  1  2  3  4
        let input = vec![1, 2, 3, 4, 5];
        let changes = vec![MoveElement {
            remove_from_idx: 1,
            insert_to_idx: 3,
        }];
        let mut tracking = vec![0, 1, 2, 3, 4];

        let expected = vec![1, 3, 4, 2, 5];
        assert_eq!(apply_vec(&input, &changes, &mut tracking), expected);
        assert_eq!(tracking, vec![0, 1, 1, 2, 4]);
    }

    #[test]
    fn test_multiple_moves() {
        let input = vec![1, 2, 3, 4, 5];
        let changes = vec![
            MoveElement {
                remove_from_idx: 1,
                insert_to_idx: 3,
            },
            MoveElement {
                remove_from_idx: 4,
                insert_to_idx: 0,
            },
        ];
        let mut tracking = vec![0, 1, 2, 3, 4];
        let expected = vec![1, 5, 3, 4, 2];
        assert_eq!(apply_vec(&input, &changes, &mut tracking), expected);
        assert_eq!(tracking, vec![0, 2, 2, 3, 5]);
    }

    #[test]
    fn test_duplicate_insert_indices() {
        let input = vec![1, 2, 3, 4, 5];
        let changes = vec![
            MoveElement {
                remove_from_idx: 1,
                insert_to_idx: 3,
            },
            MoveElement {
                remove_from_idx: 4,
                insert_to_idx: 3,
            },
        ];
        let mut tracking = vec![0, 1, 2, 3, 4];
        let expected = vec![1, 3, 4, 5, 2];
        assert_eq!(apply_vec(&input, &changes, &mut tracking), expected);
        assert_eq!(tracking, vec![0, 1, 1, 2, 5]);
    }

    #[test]
    fn test_large_input() {
        let input = (0..10).collect::<Vec<_>>();
        let changes = vec![
            MoveElement {
                remove_from_idx: 1,
                insert_to_idx: 5,
            },
            MoveElement {
                remove_from_idx: 2,
                insert_to_idx: 8,
            },
            MoveElement {
                remove_from_idx: 3,
                insert_to_idx: 9,
            },
        ];
        let mut tracking = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

        assert_eq!(apply_vec(&input, &changes, &mut tracking), vec![
            0, 4, 5, 1, 6, 7, 8, 2, 9, 3
        ]);
        assert_eq!(tracking, vec![0, 1, 1, 1, 1, 2, 4, 5, 6, 8, 10]);

        let mut tracking = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        assert_eq!(apply_vec(&input, &changes, &mut tracking), vec![
            0, 4, 5, 1, 6, 7, 8, 2, 9, 3
        ]);
        assert_eq!(tracking, vec![0, 1, 1, 1, 1, 2, 4, 5, 6, 8]);
    }

    #[test]
    fn test_empty_input() {
        let input: Vec<usize> = vec![];
        let changes = vec![];
        let mut tracking: Vec<usize> = vec![];
        let expected = vec![];
        assert_eq!(apply_vec(&input, &changes, &mut tracking), expected);
        assert_eq!(tracking, vec![]);
    }

    #[test]
    #[should_panic(expected = "attempt to move element to invalid index")]
    fn test_move_to_out_of_bounds_index() {
        let input = vec![1, 2, 3, 4, 5];
        let changes = vec![MoveElement {
            remove_from_idx: 1,
            insert_to_idx: 10,
        }];
        let mut tracking = vec![0, 1, 2, 3, 4];

        apply_vec(&input, &changes, &mut tracking);
    }

    #[test]
    #[should_panic(expected = "attempt to move element from invalid index")]
    fn test_move_from_out_of_bounds_index() {
        let input = vec![1, 2, 3, 4, 5];
        let changes = vec![MoveElement {
            remove_from_idx: 10,
            insert_to_idx: 3,
        }];
        let mut tracking = vec![0, 1, 2, 3, 4];

        apply_vec(&input, &changes, &mut tracking);
    }

    #[test]
    fn test_empty_tracking() {
        let input = vec![1, 2, 3, 4, 5];
        let changes = vec![
            MoveElement {
                remove_from_idx: 1,
                insert_to_idx: 3,
            },
            MoveElement {
                remove_from_idx: 4,
                insert_to_idx: 0,
            },
        ];
        let mut tracking: Vec<usize> = vec![];
        let expected = vec![1, 5, 3, 4, 2];
        assert_eq!(apply_vec(&input, &changes, &mut tracking), expected);
        assert_eq!(tracking, vec![]);
    }

    #[test]
    fn test_move_first_element_to_end() {
        let input = vec![1, 2, 3, 4, 5];
        let changes = vec![MoveElement {
            remove_from_idx: 0,
            insert_to_idx: 4,
        }];
        let mut tracking = vec![0, 1, 2, 3, 4];
        let expected = vec![2, 3, 4, 5, 1];
        assert_eq!(apply_vec(&input, &changes, &mut tracking), expected);
        assert_eq!(tracking, vec![0, 0, 1, 2, 3]);
    }
    #[test]
    fn test_swap_first_and_last_elements() {
        // Initial setup with elements in a sequential order for clarity.
        let input = vec![1, 2, 3, 4, 5];
        // Defining the changes to swap the first (index 0) and the last (index 4) elements.
        let changes = vec![
            MoveElement {
                remove_from_idx: 0,
                insert_to_idx: 4,
            },
            MoveElement {
                remove_from_idx: 4,
                insert_to_idx: 0,
            },
        ];
        // Tracking vector to observe changes in indices due to swaps.
        let mut tracking = vec![0, 1, 2, 3, 4];

        // Expected result after swapping the first and last elements.
        let expected = vec![5, 2, 3, 4, 1];
        // Applying the vector changes and comparing the result to the expected vector.
        assert_eq!(apply_vec(&input, &changes, &mut tracking), expected);
        // Expected tracking after the swaps, showing how indices should have adjusted.
        assert_eq!(tracking, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn test_multiple_ends() {
        // Initial setup with elements in a sequential order for clarity.
        let input = vec![1, 2, 3, 4, 5];
        // Defining the changes to swap the first (index 0) and the last (index 4) elements.
        let changes = vec![
            MoveElement {
                remove_from_idx: 0,
                insert_to_idx: 4,
            },
            MoveElement {
                remove_from_idx: 4,
                insert_to_idx: 0,
            },
            MoveElement {
                remove_from_idx: 3,
                insert_to_idx: 0,
            },
        ];
        // Tracking vector to observe changes in indices due to swaps.
        let mut tracking = vec![0, 1, 2, 3, 4, 5];

        // Expected result after swapping the first and last elements.
        let expected = vec![4, 5, 2, 3, 1];
        // Applying the vector changes and comparing the result to the expected vector.
        assert_eq!(apply_vec(&input, &changes, &mut tracking), expected);
        // Expected tracking after the swaps, showing how indices should have adjusted.
        assert_eq!(tracking, vec![0, 2, 3, 4, 4, 5]);
    }
}
