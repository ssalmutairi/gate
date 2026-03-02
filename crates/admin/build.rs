use std::path::Path;

fn main() {
    let dashboard_dir = Path::new("../../dashboard/dist");
    if !dashboard_dir.exists() {
        eprintln!();
        eprintln!("  ERROR: dashboard/dist/ not found!");
        eprintln!();
        eprintln!("  The admin binary embeds the dashboard UI at compile time.");
        eprintln!("  Build it first:");
        eprintln!();
        eprintln!("    cd dashboard && npm ci && npm run build");
        eprintln!();
        panic!("dashboard/dist/ directory is missing — see instructions above");
    }
    println!("cargo::rerun-if-changed=../../dashboard/dist");
}
