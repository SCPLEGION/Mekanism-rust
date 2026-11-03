//! Clicks on a list of item slots with a cursor stack (`AbstractContainerMenu.doClick`: PICKUP and
//! QUICK_MOVE), for screens that are not a vanilla menu (the Robit's inventory).

/// What a stack is, to the engine.
pub trait Stack: Clone {
    fn count(&self) -> u32;
    fn set_count(&mut self, n: u32);
    /// Whether the two can merge (same item and components).
    fn same(&self, other: &Self) -> bool;
    fn max(&self) -> u32;
}

/// What a click does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    Left,
    Right,
    Shift,
}

/// A slot's rule: whether `stack` may go in slot `index`.
pub type Allowed<'a, S> = &'a dyn Fn(usize, &S) -> bool;

fn put<S: Stack>(slot: &mut Option<S>, mut stack: S, index: usize, allowed: Allowed<S>, limit: Option<u32>) -> Option<S> {
    if !allowed(index, &stack) {
        return Some(stack);
    }
    let cap = limit.unwrap_or(u32::MAX).min(stack.max());
    match slot {
        Some(have) if have.same(&stack) => {
            let room = cap.saturating_sub(have.count());
            let moved = room.min(stack.count());
            have.set_count(have.count() + moved);
            stack.set_count(stack.count() - moved);
        }
        Some(_) => return Some(stack),
        None => {
            let moved = cap.min(stack.count());
            let mut placed = stack.clone();
            placed.set_count(moved);
            *slot = Some(placed);
            stack.set_count(stack.count() - moved);
        }
    }
    (stack.count() > 0).then_some(stack)
}

/// One click on `slots[index]` with the cursor holding `cursor`. For a shift click, `groups` says which slots
/// the stack moves into: the other group, first merging, then empty slots (`moveItemStackTo`).
pub fn click<S: Stack>(slots: &mut [Option<S>], cursor: &mut Option<S>, index: usize, how: Click, allowed: Allowed<S>, other_group: &[usize]) {
    if index >= slots.len() {
        return;
    }
    match how {
        Click::Shift => {
            let Some(mut stack) = slots[index].take() else {
                return;
            };
            // Merge into stacks that fit, then into empty slots.
            for pass in [true, false] {
                for &i in other_group {
                    if i == index || i >= slots.len() || slots[i].is_some() != pass {
                        continue;
                    }
                    match put(&mut slots[i], stack, i, allowed, None) {
                        Some(rest) => stack = rest,
                        None => return,
                    }
                }
            }
            slots[index] = Some(stack);
        }
        Click::Left | Click::Right => {
            let right = how == Click::Right;
            match (cursor.take(), slots[index].take()) {
                (None, None) => {}
                (None, Some(mut have)) => {
                    let take = if right { have.count().div_ceil(2) } else { have.count() };
                    let mut held = have.clone();
                    held.set_count(take);
                    have.set_count(have.count() - take);
                    *cursor = Some(held);
                    slots[index] = (have.count() > 0).then_some(have);
                }
                (Some(held), None) => {
                    let one = if right { Some(1) } else { None };
                    let rest = put(&mut slots[index], held, index, allowed, one);
                    *cursor = rest;
                }
                (Some(held), Some(have)) => {
                    if have.same(&held) {
                        slots[index] = Some(have);
                        let one = if right { Some(slots[index].as_ref().map_or(0, Stack::count) + 1) } else { None };
                        *cursor = put(&mut slots[index], held, index, allowed, one);
                    } else if allowed(index, &held) {
                        slots[index] = Some(held);
                        *cursor = Some(have);
                    } else {
                        slots[index] = Some(have);
                        *cursor = Some(held);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct S(&'static str, u32);

    impl Stack for S {
        fn count(&self) -> u32 {
            self.1
        }
        fn set_count(&mut self, n: u32) {
            self.1 = n;
        }
        fn same(&self, other: &Self) -> bool {
            self.0 == other.0
        }
        fn max(&self) -> u32 {
            64
        }
    }

    fn any(_: usize, _: &S) -> bool {
        true
    }

    #[test]
    fn left_picks_up_and_places_down() {
        let mut slots = vec![Some(S("a", 10)), None];
        let mut cursor = None;
        click(&mut slots, &mut cursor, 0, Click::Left, &any, &[]);
        assert_eq!((slots[0].clone(), cursor.clone()), (None, Some(S("a", 10))));
        click(&mut slots, &mut cursor, 1, Click::Left, &any, &[]);
        assert_eq!((slots[1].clone(), cursor.clone()), (Some(S("a", 10)), None));
    }

    #[test]
    fn right_takes_half_and_places_one() {
        let mut slots = vec![Some(S("a", 5)), None];
        let mut cursor = None;
        click(&mut slots, &mut cursor, 0, Click::Right, &any, &[]);
        assert_eq!((slots[0].clone(), cursor.clone()), (Some(S("a", 2)), Some(S("a", 3))));
        click(&mut slots, &mut cursor, 1, Click::Right, &any, &[]);
        assert_eq!((slots[1].clone(), cursor.clone()), (Some(S("a", 1)), Some(S("a", 2))));
    }

    #[test]
    fn different_stacks_swap_and_same_ones_merge_to_the_max() {
        let mut slots = vec![Some(S("a", 60)), Some(S("b", 3))];
        let mut cursor = Some(S("a", 10));
        click(&mut slots, &mut cursor, 0, Click::Left, &any, &[]);
        assert_eq!((slots[0].clone(), cursor.clone()), (Some(S("a", 64)), Some(S("a", 6))));
        click(&mut slots, &mut cursor, 1, Click::Left, &any, &[]);
        assert_eq!((slots[1].clone(), cursor.clone()), (Some(S("a", 6)), Some(S("b", 3))));
    }

    #[test]
    fn shift_moves_to_the_other_group_merging_first() {
        let mut slots = vec![Some(S("a", 10)), None, Some(S("a", 60)), None];
        let mut cursor = None;
        click(&mut slots, &mut cursor, 0, Click::Shift, &any, &[1, 2, 3]);
        assert_eq!(slots, vec![None, Some(S("a", 6)), Some(S("a", 64)), None]);
    }

    #[test]
    fn a_slot_that_refuses_keeps_the_stack_with_the_cursor() {
        let only_b = |_: usize, s: &S| s.0 == "b";
        let mut slots = vec![None];
        let mut cursor = Some(S("a", 4));
        click(&mut slots, &mut cursor, 0, Click::Left, &only_b, &[]);
        assert_eq!((slots[0].clone(), cursor), (None, Some(S("a", 4))));
    }
}
