#[cfg(target_arch = "wasm32")]
const STORAGE_KEY: &str = "gtl.viewer.guided-tours.seen";
const TOURS_MAX: usize = 32;
const ID_BYTES_MAX: usize = 64;
const STORAGE_BYTES_MAX: usize = TOURS_MAX * (ID_BYTES_MAX + 1);

#[derive(Default)]
pub(super) struct SeenGuidedTours(Vec<String>);

impl SeenGuidedTours {
    fn decode(value: &str) -> Self {
        let mut seen = Self::default();
        if value.len() > STORAGE_BYTES_MAX {
            return seen;
        }
        for id in value.lines().take(TOURS_MAX) {
            seen.insert(id.trim());
        }
        seen
    }

    pub(super) fn contains(&self, id: &str) -> bool {
        self.0.iter().any(|seen| seen == id)
    }

    fn insert(&mut self, id: &str) {
        if id.is_empty()
            || id.len() > ID_BYTES_MAX
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return;
        }
        self.0.retain(|seen| seen != id);
        if self.0.len() == TOURS_MAX {
            self.0.remove(0);
        }
        self.0.push(id.to_owned());
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn encode(&self) -> String {
        self.0.join("\n")
    }
}

#[cfg(target_arch = "wasm32")]
pub(super) fn read() -> SeenGuidedTours {
    storage()
        .and_then(|storage| storage.get_item(STORAGE_KEY).ok().flatten())
        .map_or_else(SeenGuidedTours::default, |value| {
            SeenGuidedTours::decode(&value)
        })
}

#[cfg(target_arch = "wasm32")]
pub(super) fn record(id: &str) {
    let Some(storage) = storage() else {
        return;
    };
    let mut seen = read();
    seen.insert(id);
    let _ = storage.set_item(STORAGE_KEY, &seen.encode());
}

#[cfg(target_arch = "wasm32")]
fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn read() -> SeenGuidedTours {
    SeenGuidedTours::decode("")
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn record(_id: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_storage_is_ignored_and_valid_tours_survive() {
        let mut seen = SeenGuidedTours::decode("projects\n\n{broken json}\n  files  \nprojects\n");
        seen.insert("commits");
        assert_eq!(seen.encode(), "files\nprojects\ncommits");
        assert!(!seen.contains("settings"));
        assert_eq!(
            SeenGuidedTours::decode(&"x".repeat(STORAGE_BYTES_MAX + 1)).encode(),
            ""
        );
    }

    #[test]
    fn reads_and_writes_are_bounded_and_keep_the_latest_tour() {
        let stored = (0..TOURS_MAX + 8)
            .map(|index| format!("tour-{index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut seen = SeenGuidedTours::decode(&stored);
        assert_eq!(seen.0.len(), TOURS_MAX);
        seen.insert("projects");
        assert_eq!(seen.0.len(), TOURS_MAX);
        assert!(seen.contains("projects"));
        assert!(!seen.contains("tour-0"));
        assert_eq!(
            SeenGuidedTours::decode(&seen.encode()).encode(),
            seen.encode()
        );
        seen.insert(&"a".repeat(ID_BYTES_MAX + 1));
        assert!(seen.encode().len() <= STORAGE_BYTES_MAX);
    }
}
