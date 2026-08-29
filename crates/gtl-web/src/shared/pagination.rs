use std::{num::NonZeroUsize, ops::Range};

use dioxus::prelude::*;

#[derive(Clone, Copy)]
pub(crate) struct PaginationState {
    pub(crate) item_range: Memo<Range<usize>>,
    pub(crate) previous_page_number: Memo<Option<NonZeroUsize>>,
    pub(crate) next_page_number: Memo<Option<NonZeroUsize>>,
    pub(crate) on_previous: EventHandler<()>,
    pub(crate) on_next: EventHandler<()>,
}

pub(crate) fn use_pagination(item_count: usize, page_size: NonZeroUsize) -> PaginationState {
    let mut page_number = use_signal(|| NonZeroUsize::MIN);
    let page_count = pagination_page_count(item_count, page_size);
    let item_range = use_memo(move || pagination_item_range(item_count, page_size, page_number()));
    let previous_page_number = use_memo(move || pagination_previous_page_number(page_number()));
    let next_page_number = use_memo(move || pagination_next_page_number(page_number(), page_count));
    let on_previous = use_callback(move |()| {
        if let Some(previous_page_number) = *previous_page_number.peek() {
            page_number.set(previous_page_number);
        }
    });
    let on_next = use_callback(move |()| {
        if let Some(next_page_number) = *next_page_number.peek() {
            page_number.set(next_page_number);
        }
    });

    PaginationState {
        item_range,
        previous_page_number,
        next_page_number,
        on_previous,
        on_next,
    }
}

fn pagination_page_count(item_count: usize, page_size: NonZeroUsize) -> NonZeroUsize {
    NonZeroUsize::new(item_count.div_ceil(page_size.get())).unwrap_or(NonZeroUsize::MIN)
}

fn pagination_item_range(
    item_count: usize,
    page_size: NonZeroUsize,
    page_number: NonZeroUsize,
) -> Range<usize> {
    let item_start = page_number
        .get()
        .saturating_sub(1)
        .saturating_mul(page_size.get())
        .min(item_count);
    let item_end = item_start.saturating_add(page_size.get()).min(item_count);
    item_start..item_end
}

fn pagination_previous_page_number(page_number: NonZeroUsize) -> Option<NonZeroUsize> {
    NonZeroUsize::new(page_number.get().saturating_sub(1))
}

fn pagination_next_page_number(
    page_number: NonZeroUsize,
    page_count: NonZeroUsize,
) -> Option<NonZeroUsize> {
    page_number
        .get()
        .checked_add(1)
        .and_then(NonZeroUsize::new)
        .filter(|next_page_number| *next_page_number <= page_count)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use super::{
        pagination_item_range, pagination_next_page_number, pagination_page_count,
        pagination_previous_page_number,
    };

    fn page_size() -> Result<NonZeroUsize, &'static str> {
        NonZeroUsize::new(15).ok_or("page size must be nonzero")
    }

    #[test]
    fn first_page_starts_at_zero_and_disables_back_navigation() -> Result<(), &'static str> {
        let page_size = page_size()?;
        let page_number = NonZeroUsize::MIN;
        let page_count = pagination_page_count(31, page_size);

        assert_eq!(pagination_item_range(31, page_size, page_number), 0..15);
        assert_eq!(pagination_previous_page_number(page_number), None);
        assert_eq!(
            pagination_next_page_number(page_number, page_count).map(NonZeroUsize::get),
            Some(2)
        );
        Ok(())
    }

    #[test]
    fn final_page_bounds_items_and_forward_navigation() -> Result<(), &'static str> {
        let page_size = page_size()?;
        let page_number = NonZeroUsize::new(3).ok_or("page must be nonzero")?;
        let page_count = pagination_page_count(31, page_size);

        assert_eq!(page_count.get(), 3);
        assert_eq!(pagination_item_range(31, page_size, page_number), 30..31);
        assert_eq!(
            pagination_previous_page_number(page_number).map(NonZeroUsize::get),
            Some(2)
        );
        assert_eq!(pagination_next_page_number(page_number, page_count), None);
        Ok(())
    }

    #[test]
    fn middle_page_exposes_both_adjacent_pages() -> Result<(), &'static str> {
        let page_size = page_size()?;
        let page_number = NonZeroUsize::new(2).ok_or("page must be nonzero")?;
        let page_count = pagination_page_count(31, page_size);

        assert_eq!(pagination_item_range(31, page_size, page_number), 15..30);
        assert_eq!(
            pagination_previous_page_number(page_number).map(NonZeroUsize::get),
            Some(1)
        );
        assert_eq!(
            pagination_next_page_number(page_number, page_count).map(NonZeroUsize::get),
            Some(3)
        );
        Ok(())
    }

    #[test]
    fn empty_collection_keeps_one_empty_page() -> Result<(), &'static str> {
        let page_size = page_size()?;
        let page_number = NonZeroUsize::MIN;
        let page_count = pagination_page_count(0, page_size);

        assert_eq!(page_count.get(), 1);
        assert_eq!(pagination_item_range(0, page_size, page_number), 0..0);
        assert_eq!(pagination_previous_page_number(page_number), None);
        assert_eq!(pagination_next_page_number(page_number, page_count), None);
        Ok(())
    }
}
