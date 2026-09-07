use std::{collections::BTreeMap, time::Duration};

use wasm_bindgen::JsCast as _;
use web_sys::HtmlElement;

pub(super) fn visible() -> bool {
    web_sys::window()
        .and_then(|window| window.document())
        .is_some_and(|document| !document.hidden())
}

fn cards() -> Vec<HtmlElement> {
    let Some(nodes) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.query_selector_all("[data-project-card]").ok())
    else {
        return Vec::new();
    };
    (0..nodes.length())
        .filter_map(|index| nodes.item(index)?.dyn_into::<HtmlElement>().ok())
        .collect()
}

pub(super) fn positions() -> BTreeMap<String, (f64, f64)> {
    cards()
        .into_iter()
        .filter_map(|card| {
            let key = card.get_attribute("data-project-card")?;
            let rect = card.get_bounding_client_rect();
            Some((key, (rect.x(), rect.y())))
        })
        .collect()
}

pub(super) async fn animate(previous: BTreeMap<String, (f64, f64)>) {
    if previous.is_empty() {
        return;
    }
    dioxus_sdk_time::sleep(Duration::from_millis(16)).await;
    let mut moved = Vec::new();
    for card in cards() {
        let Some((x, y)) = card
            .get_attribute("data-project-card")
            .and_then(|key| previous.get(&key).copied())
        else {
            continue;
        };
        let rect = card.get_bounding_client_rect();
        let (x, y) = (x - rect.x(), y - rect.y());
        if x.abs() < 1.0 && y.abs() < 1.0 {
            continue;
        }
        let _ = card
            .style()
            .set_property("--project-offset-x", &format!("{x}px"));
        let _ = card
            .style()
            .set_property("--project-offset-y", &format!("{y}px"));
        let _ = card.set_attribute("data-project-moving", "");
        moved.push(card);
    }
    dioxus_sdk_time::sleep(Duration::from_millis(200)).await;
    for card in moved {
        let _ = card.remove_attribute("data-project-moving");
    }
}
