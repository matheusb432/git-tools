use std::{num::NonZeroUsize, ops::Range};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CatalogPage {
    item_count: usize,
    page_size: NonZeroUsize,
    page_number: NonZeroUsize,
}

impl CatalogPage {
    pub(super) const fn first(item_count: usize, page_size: NonZeroUsize) -> Self {
        Self {
            item_count,
            page_size,
            page_number: NonZeroUsize::MIN,
        }
    }

    pub(super) const fn page_number(self) -> NonZeroUsize {
        self.page_number
    }

    pub(super) fn page_count(self) -> NonZeroUsize {
        NonZeroUsize::new(self.item_count.div_ceil(self.page_size.get()))
            .unwrap_or(NonZeroUsize::MIN)
    }

    pub(super) fn item_range(self) -> Range<usize> {
        let item_start = self
            .page_number
            .get()
            .saturating_sub(1)
            .saturating_mul(self.page_size.get())
            .min(self.item_count);
        let item_end = item_start
            .saturating_add(self.page_size.get())
            .min(self.item_count);
        item_start..item_end
    }

    pub(super) fn previous(self) -> Option<Self> {
        NonZeroUsize::new(self.page_number.get().saturating_sub(1)).map(|page_number| Self {
            page_number,
            ..self
        })
    }

    pub(super) fn next(self) -> Option<Self> {
        self.page_number
            .get()
            .checked_add(1)
            .and_then(NonZeroUsize::new)
            .filter(|page_number| *page_number <= self.page_count())
            .map(|page_number| Self {
                page_number,
                ..self
            })
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use super::CatalogPage;

    fn page_size() -> NonZeroUsize {
        NonZeroUsize::new(15).unwrap_or(NonZeroUsize::MIN)
    }

    #[test]
    fn first_page_bounds_items_and_has_only_a_forward_transition() {
        let page = CatalogPage::first(31, page_size());

        assert_eq!(page.page_number().get(), 1);
        assert_eq!(page.page_count().get(), 3);
        assert_eq!(page.item_range(), 0..15);
        assert_eq!(page.previous(), None);
        assert_eq!(page.next().map(|next| next.page_number().get()), Some(2));
    }

    #[test]
    fn final_page_bounds_items_and_has_only_a_backward_transition() {
        let page = CatalogPage::first(31, page_size())
            .next()
            .and_then(CatalogPage::next)
            .unwrap_or_else(|| CatalogPage::first(31, page_size()));

        assert_eq!(page.page_number().get(), 3);
        assert_eq!(page.item_range(), 30..31);
        assert_eq!(
            page.previous().map(|previous| previous.page_number().get()),
            Some(2)
        );
        assert_eq!(page.next(), None);
    }

    #[test]
    fn empty_catalog_keeps_one_empty_page() {
        let page = CatalogPage::first(0, page_size());

        assert_eq!(page.page_count().get(), 1);
        assert_eq!(page.item_range(), 0..0);
        assert_eq!(page.previous(), None);
        assert_eq!(page.next(), None);
    }
}
