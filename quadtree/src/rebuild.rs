use std::{cmp::Reverse, iter::Peekable, ops::AddAssign};

use num_traits::PrimInt;

#[derive(Copy, Clone)]
struct MoveElement {
    remove_from_idx: usize,
    insert_to_idx: usize,
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
                if removal.remove_from_idx < insertion.insert_to_idx {
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

#[allow(clippy::indexing_slicing)]
#[allow(dead_code)]
fn rebuild_vec<T, Idx>(
    input: Vec<T>,
    changes: Vec<MoveElement>,
    tracking: &mut [Idx], // 3 9 10
) -> Vec<T>
where
    T: Copy,
    Idx: PrimInt + AddAssign,
{
    // todo: assert tracking sorted

    let len = input.len();
    let mut result = Vec::with_capacity(len);
    let ordered_events = OrderedEvents::from(changes);

    let mut tracking = tracking.iter_mut();
    let mut tracking = TrackingUpdate::new(tracking);

    let mut src_idx = 0;
    let mut offset = 0isize;

    for event in ordered_events {
        match event {
            Event::Removal(removal) => {
                result.extend_from_slice(&input[src_idx..removal]);
                src_idx = removal + 1;

                tracking.update(removal, offset);

                offset -= 1;
            }
            Event::Insert { from, to } => {
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

        assert_eq!(rebuild_vec(input, changes, &mut tracking), expected);
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
        assert_eq!(rebuild_vec(input, changes, &mut tracking), expected);
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
        assert_eq!(rebuild_vec(input, changes, &mut tracking), expected);
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
        assert_eq!(rebuild_vec(input, changes, &mut tracking), expected);
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

        assert_eq!(
            rebuild_vec(input.clone(), changes.clone(), &mut tracking),
            vec![0, 4, 5, 1, 6, 7, 8, 2, 9, 3]
        );
        assert_eq!(tracking, vec![0, 1, 1, 1, 1, 2, 4, 5, 6, 8, 10]);

        let mut tracking = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        assert_eq!(rebuild_vec(input, changes, &mut tracking), vec![
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
        assert_eq!(rebuild_vec(input, changes, &mut tracking), expected);
        assert_eq!(tracking, vec![]);
    }
}
