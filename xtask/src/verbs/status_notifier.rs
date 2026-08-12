//! Minimal private-session `StatusNotifierWatcher` used by the Linux desktop E2E harness.

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use zbus::{blocking::Connection, message::Header};

#[derive(Debug, Clone)]
struct WatcherInterface {
    items: Arc<Mutex<Vec<String>>>,
    hosts: Arc<Mutex<Vec<String>>>,
}

struct StatusNotifierCaller(Option<String>);

impl From<Header<'_>> for StatusNotifierCaller {
    fn from(header: Header<'_>) -> Self {
        Self(header.sender().map(ToString::to_string))
    }
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl WatcherInterface {
    fn register_status_notifier_item(&self, service: &str, #[zbus(header)] header: Header<'_>) {
        let StatusNotifierCaller(sender) = header.into();
        let item = registered_item_id(service, sender.as_deref());
        if let Ok(mut items) = self.items.lock()
            && !items.contains(&item)
        {
            items.push(item);
        }
    }

    fn register_status_notifier_host(&self, service: String) {
        if let Ok(mut hosts) = self.hosts.lock()
            && !hosts.contains(&service)
        {
            hosts.push(service);
        }
    }

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.items
            .lock()
            .map_or_else(|_| Vec::new(), |items| items.clone())
    }

    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        !self.hosts.is_poisoned()
    }

    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        let _ = self.items.is_poisoned();
        0
    }
}

/// Owns the watcher bus name in the private session.
pub struct StatusNotifierWatcher {
    _connection: Connection,
}

impl StatusNotifierWatcher {
    /// Start a watcher on the private session bus at `address`.
    pub fn start(address: &str) -> Result<Self> {
        let items = Arc::new(Mutex::new(Vec::new()));
        let interface = WatcherInterface {
            items: Arc::clone(&items),
            hosts: Arc::new(Mutex::new(Vec::new())),
        };
        let connection = zbus::blocking::connection::Builder::address(address)
            .context("parse private D-Bus address for tray watcher")?
            .serve_at("/StatusNotifierWatcher", interface)
            .context("serve StatusNotifierWatcher interface")?
            .name("org.kde.StatusNotifierWatcher")
            .context("claim StatusNotifierWatcher bus name")?
            .build()
            .context("connect StatusNotifierWatcher to private D-Bus")?;
        Ok(Self {
            _connection: connection,
        })
    }
}

fn registered_item_id(service: &str, sender: Option<&str>) -> String {
    if service.starts_with('/') {
        format!("{}{service}", sender.unwrap_or("unknown"))
    } else {
        format!("{service}/StatusNotifierItem")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_name_uses_the_standard_item_path() {
        assert_eq!(
            registered_item_id("org.example.Item", Some(":1.8")),
            "org.example.Item/StatusNotifierItem"
        );
    }

    #[test]
    fn object_path_is_qualified_by_the_calling_unique_name() {
        assert_eq!(
            registered_item_id("/CustomItem", Some(":1.8")),
            ":1.8/CustomItem"
        );
    }
}
