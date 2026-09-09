use std::collections::{BTreeMap, BTreeSet};

pub(super) const ROWS_PER_WINDOW: usize = 64;
const ESTIMATED_ROW_HEIGHT: f64 = 20.0;
const ESTIMATED_HEADER_HEIGHT: f64 = 40.0;

#[derive(Debug, Clone)]
struct Heights {
    values: Vec<f64>,
    sums: Vec<f64>,
}

impl Heights {
    fn new(values: Vec<f64>) -> Self {
        let sums = fenwick_sums(&values);
        Self { values, sums }
    }

    fn prefix(&self, end: usize) -> f64 {
        let mut index = end.min(self.values.len());
        let mut height = 0.0;
        while index > 0 {
            height += self.sums[index];
            index &= index - 1;
        }
        height
    }

    fn total(&self) -> f64 {
        self.prefix(self.values.len())
    }

    fn set(&mut self, index: usize, height: f64) -> bool {
        let Some(previous) = self.values.get_mut(index) else {
            return false;
        };
        if !height.is_finite() || height <= 0.0 || (*previous - height).abs() < 0.25 {
            return false;
        }
        let delta = height - *previous;
        *previous = height;
        let mut index = index + 1;
        while index < self.sums.len() {
            self.sums[index] += delta;
            index += index.isolate_lowest_one();
        }
        true
    }

    fn at_offset(&self, offset: f64) -> usize {
        fenwick_offset(&self.sums, self.values.len(), offset)
    }
}

fn fenwick_offset(sums: &[f64], length: usize, offset: f64) -> usize {
    let mut index = 0;
    let mut height = 0.0;
    let mut step = sums.len().next_power_of_two();
    while step > 0 {
        let next = index + step;
        if next < sums.len() && height + sums[next] <= offset {
            height += sums[next];
            index = next;
        }
        step >>= 1;
    }
    index.min(length.saturating_sub(1))
}

fn fenwick_sums(values: &[f64]) -> Vec<f64> {
    let mut sums = vec![0.0; values.len() + 1];
    for (index, value) in values.iter().enumerate() {
        let index = index + 1;
        sums[index] += value;
        let parent = index + index.isolate_lowest_one();
        if parent < sums.len() {
            sums[parent] += sums[index];
        }
    }
    sums
}

#[derive(Debug, Clone)]
struct FileGeometry {
    header: f64,
    windows: Heights,
    expanded: bool,
}

impl FileGeometry {
    fn height(&self) -> f64 {
        1.0 + self.header
            + if self.expanded {
                self.windows.total()
            } else {
                0.0
            }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct WindowPlacement {
    pub(super) index: usize,
    pub(super) before: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PinnedRegion {
    pub(super) file: usize,
    pub(super) window: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct FileWindow {
    pub(super) file: usize,
    pub(super) windows: Vec<WindowPlacement>,
    pub(super) before_file: f64,
    pub(super) after_rows: f64,
}

#[derive(Debug, Clone)]
pub(in crate::views::diffs) struct DiffGeometry {
    files: Vec<FileGeometry>,
    heights: Heights,
}

impl DiffGeometry {
    pub(super) fn new(files: impl Iterator<Item = (usize, bool)>) -> Self {
        let files = files
            .map(|(rows, expanded)| FileGeometry {
                header: ESTIMATED_HEADER_HEIGHT,
                windows: Heights::new(
                    (0..rows.div_ceil(ROWS_PER_WINDOW))
                        .map(|window| {
                            estimated_height((rows - window * ROWS_PER_WINDOW).min(ROWS_PER_WINDOW))
                        })
                        .collect(),
                ),
                expanded,
            })
            .collect::<Vec<_>>();
        let heights = Heights::new(files.iter().map(FileGeometry::height).collect());
        Self { files, heights }
    }

    pub(super) fn expanded(&self, file: usize) -> Option<bool> {
        self.files.get(file).map(|file| file.expanded)
    }

    pub(super) fn window_height(&self, file: usize, window: usize) -> Option<f64> {
        self.files.get(file)?.windows.values.get(window).copied()
    }

    pub(super) fn header_height(&self, file: usize) -> Option<f64> {
        self.files.get(file).map(|file| file.header)
    }

    pub(super) fn total_height(&self) -> f64 {
        self.heights.total()
    }

    pub(super) fn file_offset(&self, file: usize) -> f64 {
        self.heights.prefix(file)
    }

    pub(super) fn row_offset(&self, file: usize, row: usize) -> Option<f64> {
        let geometry = self.files.get(file)?;
        Some(
            self.file_offset(file)
                + geometry.header
                + geometry.windows.prefix(row / ROWS_PER_WINDOW)
                + estimated_height(row % ROWS_PER_WINDOW),
        )
    }

    pub(super) fn set_expanded(&mut self, file: usize, expanded: bool) -> bool {
        let Some(geometry) = self.files.get_mut(file) else {
            return false;
        };
        if geometry.expanded == expanded {
            return false;
        }
        geometry.expanded = expanded;
        self.heights.set(file, geometry.height());
        true
    }

    pub(super) fn measure_header(&mut self, file: usize, height: f64) -> bool {
        let Some(geometry) = self.files.get_mut(file) else {
            return false;
        };
        if !height.is_finite() || height <= 0.0 || (height - geometry.header).abs() < 0.25 {
            return false;
        }
        geometry.header = height;
        self.heights.set(file, geometry.height());
        true
    }

    pub(super) fn measure_window(&mut self, file: usize, window: usize, height: f64) -> bool {
        let Some(geometry) = self.files.get_mut(file) else {
            return false;
        };
        if !geometry.windows.set(window, height) {
            return false;
        }
        self.heights.set(file, geometry.height());
        true
    }

    pub(super) fn visible(&self, top: f64, height: f64) -> Vec<FileWindow> {
        self.visible_with_pins(top, height, &[])
    }

    pub(super) fn visible_with_pins(
        &self,
        top: f64,
        height: f64,
        pins: &[PinnedRegion],
    ) -> Vec<FileWindow> {
        visible_with_pins(self, top, height, pins)
    }
}

fn visible_with_pins(
    geometry: &DiffGeometry,
    top: f64,
    height: f64,
    pins: &[PinnedRegion],
) -> Vec<FileWindow> {
    if geometry.files.is_empty() {
        return Vec::new();
    }
    let top = top.max(0.0);
    let bottom = top + height.max(1.0);
    let first = geometry.heights.at_offset(top);
    let last = geometry.heights.at_offset(bottom);
    let mut mounted = BTreeMap::<usize, BTreeSet<usize>>::new();
    for file in first..=last {
        let file_geometry = &geometry.files[file];
        let origin = geometry.file_offset(file) + file_geometry.header;
        let windows = mounted.entry(file).or_default();
        if file_geometry.expanded && !file_geometry.windows.values.is_empty() && bottom > origin {
            windows.extend(
                file_geometry.windows.at_offset((top - origin).max(0.0))
                    ..=file_geometry.windows.at_offset(bottom - origin),
            );
        }
    }
    for pin in pins {
        let Some(file) = geometry.files.get(pin.file) else {
            continue;
        };
        let windows = mounted.entry(pin.file).or_default();
        if file.expanded
            && let Some(window) = pin
                .window
                .filter(|window| *window < file.windows.values.len())
        {
            windows.insert(window);
        }
    }
    let mut previous_file_end = 0.0;
    mounted
        .into_iter()
        .map(|(file, windows)| {
            let file_geometry = &geometry.files[file];
            let before_file = geometry.file_offset(file) - previous_file_end;
            previous_file_end = geometry.file_offset(file + 1);
            let mut previous_window_end = 0.0;
            let windows = windows
                .into_iter()
                .map(|index| {
                    let before = file_geometry.windows.prefix(index) - previous_window_end;
                    previous_window_end = file_geometry.windows.prefix(index + 1);
                    WindowPlacement { index, before }
                })
                .collect();
            FileWindow {
                file,
                before_file,
                after_rows: if file_geometry.expanded {
                    file_geometry.windows.total() - previous_window_end
                } else {
                    0.0
                },
                windows,
            }
        })
        .collect()
}

fn estimated_height(rows: usize) -> f64 {
    // A window contains at most 64 rows, which f64 represents exactly.
    #[allow(clippy::cast_precision_loss)]
    let rows = rows.min(ROWS_PER_WINDOW) as f64;
    rows * ESTIMATED_ROW_HEIGHT
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn distant_rows_do_not_require_visiting_previous_windows() {
        let geometry = DiffGeometry::new([(20_005, true)].into_iter());
        let offset = geometry.row_offset(0, 19_968).unwrap();
        let visible = geometry.visible(offset, 500.0);
        assert_eq!(visible.len(), 1);
        assert_eq!(
            visible[0].windows,
            vec![WindowPlacement {
                index: 312,
                before: 399_360.0
            }]
        );
    }

    #[test]
    fn wrapped_rows_and_folding_preserve_other_file_offsets() {
        let mut geometry = DiffGeometry::new([(128, true), (64, true), (10, true)].into_iter());
        let before = geometry.file_offset(1);
        assert!(geometry.measure_window(0, 0, 1_600.0));
        assert_eq!(geometry.file_offset(1), before + 320.0);
        assert!(geometry.set_expanded(0, false));
        assert_eq!(geometry.file_offset(1), 41.0);
        assert_eq!(geometry.visible(45.0, 300.0)[0].file, 1);
        assert!(geometry.set_expanded(0, true));
        assert_eq!(geometry.file_offset(1), before + 320.0);
    }

    #[test]
    fn many_files_mount_only_the_headers_and_rows_in_the_requested_region() {
        let geometry = DiffGeometry::new(std::iter::repeat_n((64, true), 50));
        let first = geometry.visible(0.0, 900.0);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].file, 0);
        let last = geometry.visible(geometry.file_offset(49), 900.0);
        assert_eq!(last.len(), 1);
        assert_eq!(last[0].file, 49);
    }

    #[test]
    fn selection_endpoints_leave_intervening_windows_unmounted() {
        let geometry = DiffGeometry::new([(20_005, true)].into_iter());
        let pins = [
            PinnedRegion {
                file: 0,
                window: Some(0),
            },
            PinnedRegion {
                file: 0,
                window: Some(312),
            },
        ];
        let visible =
            geometry.visible_with_pins(geometry.row_offset(0, 9_600).unwrap(), 500.0, &pins);
        assert_eq!(
            visible[0]
                .windows
                .iter()
                .map(|window| window.index)
                .collect::<Vec<_>>(),
            vec![0, 150, 312]
        );
        let height = visible[0]
            .windows
            .iter()
            .map(|window| window.before + geometry.window_height(0, window.index).unwrap())
            .sum::<f64>()
            + visible[0].after_rows;
        assert_eq!(height + 41.0, geometry.total_height());
    }

    #[test]
    fn pinned_files_preserve_document_height_without_mounting_the_gap() {
        let geometry = DiffGeometry::new(std::iter::repeat_n((64, true), 50));
        let visible = geometry.visible_with_pins(
            geometry.file_offset(49),
            900.0,
            &[PinnedRegion {
                file: 0,
                window: Some(0),
            }],
        );
        assert_eq!(
            visible.iter().map(|file| file.file).collect::<Vec<_>>(),
            vec![0, 49]
        );
        assert_eq!(
            visible[1].before_file,
            geometry.file_offset(49) - geometry.file_offset(1)
        );
    }
}
