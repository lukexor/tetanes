use std::{env, fs, io, path::PathBuf, process::Command};

/// Update the homebrew formula.
fn main() -> io::Result<()> {
    println!("updating homebrew formula...");

    let cargo_target_dir =
        PathBuf::from(env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string()));
    let build_dir = cargo_target_dir.join("homebrew");
    let version = env::args()
        .next()
        .expect("must provide a version, e.g. 0.10.0");

    Command::new("gh")
        .args(["release", "download"])
        .arg(format!("v{version}"))
        .args(["--pattern", "*-apple.tar.gz", "--dir"])
        .arg(&build_dir)
        .spawn()?
        .wait()?;

    let x86_64_sha =
        fs::read_to_string(build_dir.join("tetanes-0.10.0-x86_64-apple.tar.gz-sha256.txt"))?;
    let x86_64_sha = x86_64_sha
        .split_whitespace()
        .next()
        .expect("missing sha256");
    let aarch64_sha =
        fs::read_to_string(build_dir.join("tetanes-0.10.0-aarch64-apple.tar.gz-sha256.txt"))?;
    let aarch64_sha = aarch64_sha
        .split_whitespace()
        .next()
        .expect("missing sha256");

    Command::new("git")
        .current_dir("homebrew-formulae")
        .arg("pull")
        .spawn()?
        .wait()?;

    let mut formula = fs::read_to_string("homebrew-formulae/tetanes.rb.tmpl")?;
    formula = formula.replace("%VERSION%", &version);
    formula = formula.replace("%x86_64_SHA%", x86_64_sha);
    formula = formula.replace("%aarch64_SHA%", aarch64_sha);
    fs::write("homebrew-formulae/Formula/tetanes.rb", formula)?;

    Ok(())
}
