use std::path::Path;
use std::process::Command;
use std::{fs, io};

fn main() {
    // Tell Cargo when to re-run this script
    println!("cargo:rerun-if-changed=frontend/src/");
    println!("cargo:rerun-if-changed=frontend/package.json");
    println!("cargo:rerun-if-changed=frontend/next.config.ts");
    println!("cargo:rerun-if-changed=frontend/tailwind.config.ts");

    let frontend_dir = Path::new("frontend");
    let web_dir = Path::new("web");

    if frontend_dir.exists() {
        build_frontend(frontend_dir, web_dir);
    } else if web_dir.exists() {
        // Pre-built distribution: frontend/ absent but web/ present — skip
        println!("cargo:warning=frontend/ not found but web/ exists, skipping frontend build");
    } else {
        panic!(
            "Neither frontend/ nor web/ directory exists. \
             Run `npx create-next-app@latest frontend` to create the frontend, \
             or provide a pre-built web/ directory."
        );
    }
}

fn build_frontend(frontend_dir: &Path, web_dir: &Path) {
    let npm = npm_command();

    // Install dependencies if node_modules is missing
    let node_modules = frontend_dir.join("node_modules");
    if !node_modules.exists() {
        let status = Command::new(&npm)
            .args(["install"])
            .current_dir(frontend_dir)
            .status()
            .expect("failed to run npm install");
        if !status.success() {
            panic!("npm install failed with status: {}", status);
        }
    }

    // Build the frontend (produces frontend/out/)
    let status = Command::new(&npm)
        .args(["run", "build"])
        .current_dir(frontend_dir)
        .status()
        .expect("failed to run npm run build");
    if !status.success() {
        panic!("npm run build failed with status: {}", status);
    }

    // Copy frontend/out/ to web/
    let out_dir = frontend_dir.join("out");
    if !out_dir.exists() {
        panic!("frontend/out/ was not created by the build — check next.config.ts has output: \"export\"");
    }

    // Remove old web/ directory if it exists
    if web_dir.exists() {
        fs::remove_dir_all(web_dir).expect("failed to remove old web/ directory");
    }

    // Recursively copy frontend/out/ to web/
    copy_dir_recursive(&out_dir, web_dir).expect("failed to copy frontend/out/ to web/");
}

fn npm_command() -> String {
    if cfg!(target_os = "windows") {
        "npm.cmd".to_string()
    } else {
        "npm".to_string()
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
