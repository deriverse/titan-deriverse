use std::marker::PhantomData;

use drv_models::{constants::nulls::NULL_ORDER, state::types::Order};

pub type Orders = Vec<Order>;

pub trait OrdersSugar {
    fn new_orders(slice: &[Order]) -> Self;
    fn iter_from<'a>(&'a self, start_idx: u32) -> OrdersIter<'a>;
}

pub trait OrdersMutSugar {
    fn iter_mut_from<'a>(&'a mut self, start_idx: u32) -> OrdersIterMut<'a>;
}

impl OrdersSugar for Orders {
    fn new_orders(slice: &[Order]) -> Self {
        slice.to_vec()
    }

    fn iter_from<'a>(&'a self, start_idx: u32) -> OrdersIter<'a> {
        OrdersIter {
            slice: self.as_slice(),
            current: Some(start_idx),
        }
    }
}

impl OrdersMutSugar for Orders {
    fn iter_mut_from<'a>(&'a mut self, start_idx: u32) -> OrdersIterMut<'a> {
        OrdersIterMut {
            ptr: self.as_mut_ptr(),
            current: Some(start_idx),
            _marker: PhantomData,
        }
    }
}

pub struct OrdersIter<'a> {
    slice: &'a [Order],
    current: Option<u32>,
}

impl<'a> Iterator for OrdersIter<'a> {
    type Item = (u32, Order);

    fn next(&mut self) -> Option<Self::Item> {
        let idx = match self.current.take() {
            Some(i) => i,
            None => return None,
        };

        if idx == NULL_ORDER {
            return None;
        }

        let entry = self.slice[idx as usize];

        let next_idx = entry.next;
        if next_idx == NULL_ORDER || next_idx == idx {
            self.current = None;
        } else {
            self.current = Some(next_idx);
        }

        Some((idx, entry))
    }
}

pub struct OrdersIterMut<'a> {
    ptr: *mut Order,
    current: Option<u32>,
    _marker: PhantomData<&'a ()>,
}

impl<'a> Iterator for OrdersIterMut<'a> {
    type Item = (u32, &'a mut Order);

    fn next(&mut self) -> Option<Self::Item> {
        let idx = match self.current.take() {
            Some(i) => i,
            None => return None,
        };

        if idx == NULL_ORDER {
            return None;
        }

        let entry: &'a mut Order = unsafe { &mut *self.ptr.add(idx as usize) };

        let next_idx = entry.next;
        if next_idx == NULL_ORDER || next_idx == idx {
            self.current = None;
        } else {
            self.current = Some(next_idx);
        }

        Some((idx, entry))
    }
}
