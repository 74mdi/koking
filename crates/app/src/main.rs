#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use signal_alt::App;

#[cfg(feature = "desktop")]
fn main() {
    dioxus_desktop::launch(App);
}

#[cfg(all(not(feature = "desktop"), feature = "web"))]
fn main() {
    dioxus::launch(App);
}

#[cfg(not(any(feature = "desktop", feature = "web")))]
fn main() {}
