use std::marker::PhantomData;

use drv_models::{constants::nulls::NULL_ORDER, state::types::PxOrders};

pub type Lines = Vec<PxOrders>;

pub trait LinesSugar {
    fn new_lines(slice: &[PxOrders]) -> Self;
    fn iter_from<'a>(&'a self, start_idx: u32, lines_count: usize) -> LinesIter<'a>;
}

pub trait LinesMutSugar {
    fn iter_mut_from<'a>(&'a mut self, start_idx: u32) -> LinesIterMut<'a>;
}

impl LinesSugar for Lines {
    fn new_lines(slice: &[PxOrders]) -> Self {
        slice.to_vec()
    }

    fn iter_from<'a>(&'a self, start_idx: u32, lines_count: usize) -> LinesIter<'a> {
        LinesIter {
            slice: self.as_slice(),
            current: Some(start_idx),
            remaining: lines_count,
        }
    }
}

pub struct LinesIter<'a> {
    slice: &'a [PxOrders],
    current: Option<u32>,
    remaining: usize,
}

impl<'a> Iterator for LinesIter<'a> {
    type Item = (u32, PxOrders);

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let idx = match self.current.take() {
            Some(i) => i,
            None => return None,
        };

        if idx == NULL_ORDER {
            return None;
        }

        let entry = self.slice[idx as usize];
        self.remaining = self.remaining.saturating_sub(1);

        let next_idx = entry.next;
        if next_idx == NULL_ORDER || next_idx == idx {
            self.current = None;
        } else {
            self.current = Some(next_idx);
        }

        Some((idx, entry))
    }
}

pub struct LinesIterMut<'a> {
    ptr: *mut PxOrders,
    current: Option<u32>,
    remaining: usize,
    market: PhantomData<&'a ()>,
}

impl<'a> Iterator for LinesIterMut<'a> {
    type Item = (u32, &'a mut PxOrders);

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let idx = match self.current.take() {
            Some(i) => i,
            None => return None,
        };

        if idx == NULL_ORDER {
            return None;
        }

        let entry: &'a mut PxOrders = unsafe { &mut *self.ptr.add(idx as usize) };

        self.remaining = self.remaining.saturating_sub(1);

        let next_idx = entry.next;
        if next_idx == NULL_ORDER || next_idx == idx {
            self.current = None;
        } else {
            self.current = Some(next_idx);
        }

        Some((idx, entry))
    }
}
