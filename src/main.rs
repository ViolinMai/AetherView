// Root workspace shim removed in favor of modular `crates/app`.
// Kept minimal in case someone invokes `cargo` outside workspace member specification.
fn main() {
    println!("Run `cargo run -p aetherview` to launch AetherView.");
}
