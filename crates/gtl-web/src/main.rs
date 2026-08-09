mod app;
mod entities;
mod shared;
mod views;

fn main() {
    dioxus::launch(app::App);
}
