use std::{
    num::NonZeroUsize,
    ops::Range,
    sync::{Arc, Weak},
};

use lru::LruCache;
use sha2::{Digest as _, Sha256};

use super::FileStatus;

/// Immutable UTF-8 source entries with explicit boundaries, including empty entries.
#[derive(Clone, Debug)]
pub struct DiffSourceLines {
    storage: Arc<SourceStorage>,
}

#[derive(Debug, PartialEq, Eq)]
struct SourceStorage {
    text: Box<str>,
    ends: Box<[usize]>,
    fingerprint: [u8; 32],
    status: FileStatus,
    index: gtl_parser::index::DiffIndex,
}

impl DiffSourceLines {
    #[must_use]
    pub fn len(&self) -> usize {
        self.storage.ends.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.storage.ends.is_empty()
    }

    #[must_use]
    pub fn text_bytes(&self) -> usize {
        self.storage.text.len()
    }

    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        std::mem::size_of::<SourceStorage>()
            + self.storage.text.len()
            + std::mem::size_of_val(self.storage.ends.as_ref())
            + self.storage.index.retained_bytes()
    }

    #[must_use]
    pub fn iter(&self) -> DiffSourceLinesIter<'_> {
        DiffSourceLinesIter {
            source: self,
            range: 0..self.len(),
        }
    }

    /// # Panics
    /// Panics when `chunk_size` is zero, matching slice chunking.
    pub fn chunks(&self, chunk_size: usize) -> impl Iterator<Item = DiffSourceLinesIter<'_>> {
        (0..self.len())
            .step_by(chunk_size)
            .map(move |start| DiffSourceLinesIter {
                source: self,
                range: start..start.saturating_add(chunk_size).min(self.len()),
            })
    }

    pub(super) fn status(&self) -> FileStatus {
        self.storage.status
    }

    pub(crate) fn index(&self) -> &gtl_parser::index::DiffIndex {
        &self.storage.index
    }

    pub(crate) fn line(&self, index: usize) -> &str {
        let start = index
            .checked_sub(1)
            .map_or(0, |previous| self.storage.ends[previous]);
        &self.storage.text[start..self.storage.ends[index]]
    }
}

impl Default for DiffSourceLines {
    fn default() -> Self {
        std::iter::empty::<&str>().collect()
    }
}

impl PartialEq for DiffSourceLines {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.storage, &other.storage) || self.storage == other.storage
    }
}

impl Eq for DiffSourceLines {}

impl<Line: AsRef<str>> FromIterator<Line> for DiffSourceLines {
    fn from_iter<Lines: IntoIterator<Item = Line>>(lines: Lines) -> Self {
        let lines = lines.into_iter();
        let mut text = String::new();
        let mut ends = Vec::with_capacity(lines.size_hint().0);
        let mut digest = Sha256::new();
        digest.update(b"gtl.diff.source-entries.v1\0");
        let mut renamed = false;
        let mut added = false;
        let mut deleted = false;
        for line in lines {
            let line = line.as_ref();
            renamed |= line.starts_with("rename from ") || line.starts_with("rename to ");
            added |= line.starts_with("new file ") || line == "--- /dev/null";
            deleted |= line.starts_with("deleted file ") || line == "+++ /dev/null";
            text.push_str(line);
            ends.push(text.len());
            digest.update((line.len() as u64).to_be_bytes());
            digest.update(line.as_bytes());
        }
        let index = gtl_parser::index::DiffIndex::new(ends.iter().scan(0, |start, &end| {
            let line = &text[*start..end];
            *start = end;
            Some(line)
        }));
        Self {
            storage: Arc::new(SourceStorage {
                index,
                text: text.into_boxed_str(),
                ends: ends.into_boxed_slice(),
                fingerprint: digest.finalize().into(),
                status: if renamed {
                    FileStatus::Renamed
                } else if added {
                    FileStatus::Added
                } else if deleted {
                    FileStatus::Deleted
                } else {
                    FileStatus::Modified
                },
            }),
        }
    }
}

impl From<Vec<String>> for DiffSourceLines {
    fn from(lines: Vec<String>) -> Self {
        lines.into_iter().collect()
    }
}

impl<'source> IntoIterator for &'source DiffSourceLines {
    type Item = &'source str;
    type IntoIter = DiffSourceLinesIter<'source>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Clone)]
pub struct DiffSourceLinesIter<'source> {
    source: &'source DiffSourceLines,
    range: Range<usize>,
}

impl<'source> Iterator for DiffSourceLinesIter<'source> {
    type Item = &'source str;

    fn next(&mut self) -> Option<Self::Item> {
        self.range.next().map(|index| self.source.line(index))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.range.size_hint()
    }
}

impl DoubleEndedIterator for DiffSourceLinesIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.range.next_back().map(|index| self.source.line(index))
    }
}

impl ExactSizeIterator for DiffSourceLinesIter<'_> {}

/// A bounded index that shares live allocations without retaining diff text itself.
pub(crate) struct DiffSourcePool {
    entries: LruCache<[u8; 32], Weak<SourceStorage>>,
}

impl Default for DiffSourcePool {
    fn default() -> Self {
        const MAX_ENTRIES: NonZeroUsize = NonZeroUsize::new(8192).unwrap();
        Self {
            entries: LruCache::new(MAX_ENTRIES),
        }
    }
}

impl DiffSourcePool {
    pub(crate) fn intern(&mut self, source: &mut DiffSourceLines) {
        let key = source.storage.fingerprint;
        if let Some(shared) = self.entries.get(&key).and_then(Weak::upgrade)
            && (Arc::ptr_eq(&source.storage, &shared) || source.storage == shared)
        {
            source.storage = shared;
            return;
        }
        self.entries.put(key, Arc::downgrade(&source.storage));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_sources_share_storage_without_extending_its_lifetime() {
        let mut pool = DiffSourcePool::default();
        let mut first = ["+same", "", "é\n\0"]
            .into_iter()
            .collect::<DiffSourceLines>();
        let mut second = ["+same", "", "é\n\0"]
            .into_iter()
            .collect::<DiffSourceLines>();
        pool.intern(&mut first);
        pool.intern(&mut second);
        assert!(Arc::ptr_eq(&first.storage, &second.storage));
        assert_eq!(second.iter().collect::<Vec<_>>(), ["+same", "", "é\n\0"]);
        let weak = Arc::downgrade(&first.storage);
        drop(first);
        drop(second);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn a_digest_collision_does_not_replace_distinct_source_boundaries() {
        let mut pool = DiffSourcePool::default();
        let mut first = ["ab", "c"].into_iter().collect::<DiffSourceLines>();
        let mut second = ["a", "bc"].into_iter().collect::<DiffSourceLines>();
        Arc::get_mut(&mut second.storage).unwrap().fingerprint = first.storage.fingerprint;
        pool.intern(&mut first);
        pool.intern(&mut second);
        assert!(!Arc::ptr_eq(&first.storage, &second.storage));
        assert_eq!(second.iter().collect::<Vec<_>>(), ["a", "bc"]);
    }

    #[test]
    fn expired_sources_do_not_grow_the_index_without_bound() {
        let mut pool = DiffSourcePool {
            entries: LruCache::new(NonZeroUsize::new(2).unwrap()),
        };
        for value in 0..10 {
            let mut source = [value.to_string()].into_iter().collect();
            pool.intern(&mut source);
        }
        assert_eq!(pool.entries.len(), 2);
        assert!(
            pool.entries
                .iter()
                .all(|(_, source)| source.upgrade().is_none())
        );
    }
}
