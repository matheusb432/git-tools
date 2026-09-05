use dioxus::prelude::*;
#[cfg(any(target_arch = "wasm32", test))]
use gtl_models::viewer::{ViewerTabId, ViewerTabPlacement};
use gtl_wire::viewer::MoveViewerTab;

#[cfg(target_arch = "wasm32")]
mod browser;

#[derive(Clone, Copy)]
pub(super) struct PointerDrag {
    pub(super) start: Callback<PointerEvent>,
    pub(super) move_pointer: Callback<PointerEvent>,
    pub(super) release: Callback<PointerEvent>,
    pub(super) cancel: Callback<()>,
    pub(super) suppress_click: Callback<(), bool>,
}

pub(super) fn use_pointer_drag(onmove: Option<EventHandler<MoveViewerTab>>) -> PointerDrag {
    #[cfg(target_arch = "wasm32")]
    {
        browser::use_pointer_drag(onmove)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = onmove;
        PointerDrag {
            start: use_callback(|_| {}),
            move_pointer: use_callback(|_| {}),
            release: use_callback(|_| {}),
            cancel: use_callback(|()| {}),
            suppress_click: use_callback(|()| false),
        }
    }
}

#[cfg(any(target_arch = "wasm32", test))]
struct TabSlot {
    id: ViewerTabId,
    start: f64,
    end: f64,
}

#[cfg(any(target_arch = "wasm32", test))]
struct TabDragLayout {
    slots: Vec<TabSlot>,
    source: usize,
}

#[cfg(any(target_arch = "wasm32", test))]
impl TabDragLayout {
    fn new(slots: Vec<TabSlot>, tab_id: ViewerTabId) -> Option<Self> {
        let source = slots.iter().position(|slot| slot.id == tab_id)?;
        let mut ids = std::collections::HashSet::new();
        if slots.iter().any(|slot| {
            !slot.start.is_finite()
                || !slot.end.is_finite()
                || slot.end <= slot.start
                || !ids.insert(slot.id)
        }) || slots.windows(2).any(|pair| pair[0].end > pair[1].start)
        {
            return None;
        }
        Some(Self { slots, source })
    }

    fn destination(&self, distance: f64) -> usize {
        let source = &self.slots[self.source];
        let center = source.start.midpoint(source.end) + distance;
        self.slots
            .iter()
            .enumerate()
            .filter(|(index, slot)| *index != self.source && center > slot.start.midpoint(slot.end))
            .count()
    }

    fn offsets(&self, distance: f64) -> impl Iterator<Item = f64> + '_ {
        let destination = self.destination(distance);
        let source = &self.slots[self.source];
        let gap = self
            .slots
            .windows(2)
            .next()
            .map_or(0.0, |pair| pair[1].start - pair[0].end);
        let displacement = source.end - source.start + gap;
        (0..self.slots.len()).map(move |index| match index {
            index if index > self.source && index <= destination => -displacement,
            index if index < self.source && index >= destination => displacement,
            _ => 0.0,
        })
    }

    fn request(&self, distance: f64) -> Option<MoveViewerTab> {
        let destination = self.destination(distance);
        (destination != self.source).then(|| MoveViewerTab {
            tab_id: self.slots[self.source].id,
            target_tab_id: self.slots[destination].id,
            placement: if destination < self.source {
                ViewerTabPlacement::Before
            } else {
                ViewerTabPlacement::After
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{TabDragLayout, TabSlot, ViewerTabPlacement};
    use crate::test_support::{TestResult, viewer_tab_id};

    fn slots() -> Result<Vec<TabSlot>, Box<dyn std::error::Error>> {
        [(1, 0.0, 100.0), (2, 100.0, 300.0), (3, 300.0, 420.0)]
            .into_iter()
            .map(|(id, start, end)| {
                Ok(TabSlot {
                    id: viewer_tab_id(id)?,
                    start,
                    end,
                })
            })
            .collect()
    }

    #[test]
    fn unequal_tabs_open_the_destination_gap_without_moving_hit_regions() -> TestResult {
        let layout = TabDragLayout::new(slots()?, viewer_tab_id(1)?).ok_or("layout")?;
        assert!(layout.request(0.0).is_none());
        assert_eq!(
            layout.offsets(200.0).collect::<Vec<_>>(),
            [0.0, -100.0, 0.0]
        );
        assert_eq!(
            layout.offsets(500.0).collect::<Vec<_>>(),
            [0.0, -100.0, -100.0]
        );
        let request = layout.request(500.0).ok_or("move")?;
        assert_eq!(request.target_tab_id, viewer_tab_id(3)?);
        assert_eq!(request.placement, ViewerTabPlacement::After);
        assert_eq!(layout.offsets(0.0).collect::<Vec<_>>(), [0.0; 3]);
        assert!(layout.request(0.0).is_none());
        Ok(())
    }

    #[test]
    fn reverse_drag_uses_the_source_width_and_preserves_list_gaps() -> TestResult {
        let mut slots = slots()?;
        slots[1].start += 1.0;
        slots[1].end += 1.0;
        slots[2].start += 2.0;
        slots[2].end += 2.0;
        let layout = TabDragLayout::new(slots, viewer_tab_id(3)?).ok_or("layout")?;
        assert_eq!(
            layout.offsets(-500.0).collect::<Vec<_>>(),
            [121.0, 121.0, 0.0]
        );
        let request = layout.request(-500.0).ok_or("move")?;
        assert_eq!(request.target_tab_id, viewer_tab_id(1)?);
        assert_eq!(request.placement, ViewerTabPlacement::Before);
        Ok(())
    }

    #[test]
    fn malformed_dom_snapshots_cannot_start_a_drag() -> TestResult {
        let id = viewer_tab_id(1)?;
        assert!(TabDragLayout::new(Vec::new(), id).is_none());
        let mut duplicate = slots()?;
        duplicate[1].id = id;
        assert!(TabDragLayout::new(duplicate, id).is_none());
        let mut overlapping = slots()?;
        overlapping[1].start = 99.0;
        assert!(TabDragLayout::new(overlapping, id).is_none());
        let mut hidden = slots()?;
        hidden[0].end = 0.0;
        assert!(TabDragLayout::new(hidden, id).is_none());
        Ok(())
    }
}
