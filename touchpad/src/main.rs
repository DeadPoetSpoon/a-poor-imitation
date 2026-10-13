#[cfg(target_arch = "wasm32")]
mod browser;

#[cfg(target_arch = "wasm32")]
mod dashboard;

#[cfg(target_arch = "wasm32")]
fn main() {
    browser::run();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    println!(
        "touchpad runs in a browser. Start world with --webtransport, then run `trunk serve` in touchpad/. See touchpad/README.md."
    );
}
