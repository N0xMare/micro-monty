//! Prints this build's snapshot build id in hex: what `mm version`,
//! `mm-cell-worker --version` and a stored image's bytes 10..42 show.
//! `cargo xtask package` compares it between the workspace and the
//! packaged crates.

fn main() {
    for b in micro_monty::build_id() {
        print!("{b:02x}");
    }
    println!();
}
