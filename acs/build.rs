use std::path::Path;

fn main() {
    let web_dir = Path::new("web");
    if !web_dir.exists() || !web_dir.is_dir() {
        panic!(
            "web/ directory not found. This directory should contain the static site files."
        );
    }

    println!("cargo:rerun-if-changed=web/");
}
