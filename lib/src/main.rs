#[cfg(not(debug_assertions))]
compile_error!("The watchexec CLI has moved to the watchexec-cli crate");
fn main() {
    panic!("The watchexec CLI has moved to the watchexec-cli crate");
}
