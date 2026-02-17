use anyhow::Context;
use cfg_if::cfg_if;
use clap::Parser;
use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Output},
};

/// CLI options
#[derive(Parser, Debug)]
#[must_use]
struct Args {
    /// Target platform to build for. e.g. `x86_64-unknown-linux-gnu`.
    #[clap(long)]
    target: String,
    /// Build for a target platform different from the host using
    /// `cross`. e.g. `aarch64-unknown-linux-gnu`.
    #[clap(long)]
    cross: bool,
    /// Clean `dist` directory before building.
    #[clap(long)]
    clean: bool,
}

/// Build context with required variables and platform targets.
#[derive(Debug)]
#[must_use]
struct Build {
    version: &'static str,
    bin_name: &'static str,
    bin_path: PathBuf,
    app_name: &'static str,
    arch: &'static str,
    target_arch: String,
    #[cfg(target_os = "linux")]
    cross: bool,
    cargo_target_dir: PathBuf,
    dist_dir: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let build = Build::new(args)?;

    println!("building artifacts: {build:?}...");

    if build.target_arch == "wasm32-unknown-unknown" {
        build.make(["build-web"])?;
        build.compress_web_artifacts()?;
    } else {
        let build_args = ["build", "--target", &build.target_arch];
        cfg_if! {
            if #[cfg(target_os = "linux")] {
                let build_args = if build.cross {
                    vec!["build-cross"]
                } else {
                    build_args.to_vec()
                };
                build.make(build_args)?;
                build.create_linux_artifacts()?;
            } else if #[cfg(target_os = "macos")] {
                build.make(build_args)?;
                build.create_macos_app()?;
            } else if #[cfg(target_os = "windows")] {
                build.create_windows_installer()?;
            }
        }
    }

    Ok(())
}

impl Build {
    /// Create a new build context by cleaning up any previous artifacts and ensuring the
    /// dist directory is created.
    fn new(args: Args) -> anyhow::Result<Self> {
        let bin_name = env!("CARGO_PKG_NAME");
        let dist_dir = PathBuf::from(bin_name).join("dist");

        if args.clean {
            let _ = remove_dir_all(&dist_dir); // ignore if not found
        }
        create_dir_all(&dist_dir)?;

        let cargo_target_dir =
            PathBuf::from(env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string()));
        let target_arch = args.target;

        Ok(Build {
            version: env!("CARGO_PKG_VERSION"),
            bin_name,
            bin_path: cargo_target_dir
                .join(&target_arch)
                .join("dist")
                .join(bin_name),
            app_name: "TetaNES",
            arch: if target_arch.starts_with("x86_64") {
                "x86_64"
            } else if target_arch.starts_with("aarch64") {
                "aarch64"
            } else if target_arch.starts_with("wasm32") {
                "wasm32"
            } else {
                anyhow::bail!("unsupported target_arch: {target_arch}");
            },
            target_arch,
            #[cfg(target_os = "linux")]
            cross: args.cross,
            cargo_target_dir,
            dist_dir,
        })
    }

    /// Run `cargo make` to build binary.
    ///
    /// Note: Wix on Windows bakes in the build step
    fn make(
        &self,
        args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    ) -> anyhow::Result<ExitStatus> {
        let mut cmd = Command::new("cargo");
        cmd.arg("make");
        for arg in args {
            cmd.arg(arg);
        }
        // TODO: disable lto and make pgo build
        cmd_spawn_wait(&mut cmd)
    }

    /// Create a dist directory for artifacts.
    fn create_build_dir(&self, dir: impl AsRef<Path>) -> anyhow::Result<PathBuf> {
        let build_dir = self.cargo_target_dir.join(dir);

        println!("creating build directory: {build_dir:?}");

        let _ = remove_dir_all(&build_dir); // ignore if not found
        create_dir_all(&build_dir)?;

        Ok(build_dir)
    }

    /// Write out a SHA256 checksum for a file.
    fn write_sha256(&self, file: impl AsRef<Path>, output: impl AsRef<Path>) -> anyhow::Result<()> {
        let file = file.as_ref();
        let output = output.as_ref();

        let shasum = {
            cfg_if! {
                if #[cfg(target_os = "windows")] {
                    cmd_output(Command::new("powershell")
                        .args(["-Command", "$ErrorActionPreference = 'Stop';"])
                        .arg(format!("Get-FileHash -Algorithm SHA256 {} | select-object -ExpandProperty Hash", file.display())))?
                } else {
                    cmd_output(Command::new("shasum")
                        .current_dir(file.parent().with_context(|| format!("no parent directory for {file:?}"))?)
                        .args(["-a", "256"])
                        .arg(file.file_name().with_context(|| format!("no file_name for {file:?}"))?))?
                }
            }
        };
        let sha256 = std::str::from_utf8(&shasum.stdout)
            .with_context(|| format!("invalid sha output for {file:?}"))?
            .trim()
            .to_owned();

        println!("sha256: {sha256}");

        write(output, shasum.stdout)
    }

    /// Create a Gzipped tarball.
    fn tar_gz(
        &self,
        tgz_name: impl AsRef<str>,
        directory: impl AsRef<Path>,
        files: impl IntoIterator<Item = impl AsRef<Path>>,
    ) -> anyhow::Result<()> {
        let directory = directory.as_ref();
        let tgz_name = tgz_name.as_ref();
        let tgz_path = self.dist_dir.join(tgz_name);

        let mut cmd = Command::new("tar");
        cmd.arg("-czvf")
            .arg(&tgz_path)
            .arg(format!("--directory={}", directory.display()));
        for file in files {
            cmd.arg(file.as_ref());
        }

        cmd_spawn_wait(&mut cmd)?;
        self.write_sha256(
            tgz_path,
            self.dist_dir.join(format!("{tgz_name}-sha256.txt")),
        )
    }

    /// Create linux artifacts (.tar.gz, .deb and .AppImage).
    #[cfg(target_os = "linux")]
    fn create_linux_artifacts(&self) -> anyhow::Result<()> {
        println!("creating linux artifacts...");

        let build_dir = self.create_build_dir("linux")?;

        // Binary .tar.gz
        copy("README.md", build_dir.join("README.md"))?;
        copy("LICENSE-MIT", build_dir.join("LICENSE-MIT"))?;
        copy("LICENSE-APACHE", build_dir.join("LICENSE-APACHE"))?;

        let bin_path_build = build_dir.join(self.bin_name);
        copy(&self.bin_path, &bin_path_build)?;

        self.tar_gz(
            format!(
                "{}-{}-{}-unknown-linux-gnu.tar.gz",
                self.bin_name, self.version, self.arch
            ),
            &build_dir,
            ["."],
        )?;

        // TODO: Fix deb/AppImage for cross builds
        if !self.cross {
            // Debian .deb
            // NOTE: 1- is the deb revision number
            let deb_name = format!("{}-{}-1-amd64.deb", self.bin_name, self.version);
            let deb_path_dist = self.dist_dir.join(&deb_name);
            cmd_spawn_wait(
                Command::new("cargo")
                    .args([
                        "deb",
                        "-v",
                        "-p",
                        "tetanes",
                        "--profile",
                        "dist",
                        "--target",
                        &self.target_arch,
                        "--no-build", // already built
                        "--no-strip", // already stripped
                        "-o",
                    ])
                    .arg(&deb_path_dist),
            )?;

            self.write_sha256(
                &deb_path_dist,
                self.dist_dir.join(format!("{deb_name}-sha256.txt")),
            )?;

            // AppImage
            let linuxdeploy_cmd = format!("vendored/linuxdeploy-{}.AppImage", self.arch);
            let app_dir = build_dir.join("AppDir");
            let desktop_name = format!("assets/linux/{}.desktop", self.bin_name);
            cmd_spawn_wait(
                Command::new(&linuxdeploy_cmd)
                    .arg("-e")
                    .arg(&self.bin_path)
                    .args([
                        "-i",
                        "assets/linux/icon.png",
                        "-d",
                        &desktop_name,
                        "--appdir",
                    ])
                    .arg(&app_dir)
                    .args(["--output", "appimage"]),
            )?;

            // NOTE: AppImage name is derived from tetanes.desktop
            // Rename to lowercase
            let app_image_name =
                format!("{}-{}-{}.AppImage", self.bin_name, self.version, self.arch);
            let app_image_path = PathBuf::from(format!("{}-{}.AppImage", self.app_name, self.arch));
            let app_image_path_dist = self.dist_dir.join(&app_image_name);
            rename(&app_image_path, &app_image_path_dist)?;
            self.write_sha256(
                &app_image_path_dist,
                self.dist_dir.join(format!("{app_image_name}-sha256.txt")),
            )?;
        }

        Ok(())
    }

    /// Create macOS artifacts (.app in a .tar.gz and separate .dmg).
    #[cfg(target_os = "macos")]
    fn create_macos_app(&self) -> anyhow::Result<()> {
        println!("creating macos app...");

        let build_dir = self.create_build_dir("macos")?;

        let artifact_name = format!("{}-{}-{}", self.bin_name, self.version, self.arch);
        let volume = PathBuf::from("/Volumes").join(&artifact_name);
        let app_name = format!("{}.app", self.app_name);
        let dmg_name = format!("{artifact_name}-uncompressed.dmg");
        let dmg_path = build_dir.join(dmg_name);
        let dmg_name_compressed = format!("{artifact_name}.dmg");
        let dmg_path_compressed = build_dir.join(&dmg_name_compressed);
        let dmg_path_dist = self.dist_dir.join(&dmg_name_compressed);

        if let Err(err) = cmd_status(Command::new("hdiutil").arg("detach").arg(&volume)) {
            eprintln!("failed to detach volume: {err:?}");
        }
        cmd_spawn_wait(
            Command::new("hdiutil")
                .args(["create", "-size", "50m", "-volname", &artifact_name])
                .arg(&dmg_path),
        )?;
        cmd_spawn_wait(Command::new("hdiutil").arg("attach").arg(&dmg_path))?;

        let _ = cmd_status(Command::new("mdutil").args(["-i", "off"]).arg(&volume));

        let app_dir = volume.join(&app_name);
        create_dir_all(app_dir.join("Contents/MacOS"))?;
        create_dir_all(app_dir.join("Contents/Resources"))?;
        create_dir_all(volume.join(".Picture"))?;

        println!("updating Info.plist version: {}", self.version);

        let mut info_plist = read_to_string("assets/macos/Info.plist")?;
        info_plist = info_plist.replace("%VERSION%", self.version);
        write(app_dir.join("Contents/Info.plist"), info_plist)?;

        // TODO: maybe include readme/license?
        copy(
            "assets/macos/Icon.icns",
            app_dir.join("Contents/Resources/Icon.icns"),
        )?;
        copy(
            "assets/macos/background.png",
            volume.join(".Picture/background.png"),
        )?;
        copy("assets/macos/.DS_Store", volume.join(".DS_Store"))?;
        copy(
            &self.bin_path,
            app_dir.join("Contents/MacOS").join(self.bin_name),
        )?;

        symlink("/Applications", volume.join("Applications"))?;

        println!("configuring app bundle window...");

        let app_bin_path = app_dir.join("Contents/MacOS").join(self.bin_name);
        cmd_spawn_wait(
            Command::new("codesign")
                .args(["--force", "--sign", "-"])
                .arg(&app_bin_path),
        )?;
        // TODO: fix
        // ensure spctl --assess --type execute "${VOLUME}/${APP_NAME}.app"
        cmd_spawn_wait(
            Command::new("codesign")
                .args(["--verify", "--strict", "--verbose=2"])
                .arg(&app_bin_path),
        )?;

        self.tar_gz(
            format!(
                "{}-{}-{}-apple-darwin.tar.gz",
                self.bin_name, self.version, self.arch
            ),
            &volume,
            [&app_name],
        )?;

        std::thread::sleep(std::time::Duration::from_secs(2));

        if let Err(err) = cmd_spawn_wait(
            Command::new("hdiutil")
                .args(["detach", "-force"])
                .arg(&volume),
        ) {
            eprintln!("first detach failed, retrying: {err:?}");
            std::thread::sleep(std::time::Duration::from_secs(3));
            cmd_spawn_wait(
                Command::new("hdiutil")
                    .args(["detach", "-force"])
                    .arg(&volume),
            )?;
        }

        cmd_spawn_wait(
            Command::new("hdiutil")
                .args(["convert", "-format", "UDBZ", "-o"])
                .arg(&dmg_path_compressed)
                .arg(&dmg_path),
        )?;

        rename(&dmg_path_compressed, &dmg_path_dist)?;
        self.write_sha256(
            &dmg_path_dist,
            self.dist_dir
                .join(format!("{dmg_name_compressed}-sha256.txt")),
        )
    }

    /// Create Windows artifacts (.msi).
    #[cfg(target_os = "windows")]
    fn create_windows_installer(&self) -> anyhow::Result<()> {
        println!("creating windows installer...");

        let build_dir = self.create_build_dir("windows")?;

        let artifact_name = format!("{}-{}-{}", self.bin_name, self.version, self.arch);
        let installer_name = format!("{artifact_name}.msi");
        let installer_path_build = build_dir.join(&installer_name);
        let zip_name = format!("{artifact_name}.zip");
        let zip_path_dist = self.dist_dir.join(&zip_name);

        cmd_spawn_wait(
            Command::new("cargo")
                .args([
                    "wix",
                    "-v",
                    "-p",
                    "tetanes",
                    "--profile",
                    "dist",
                    "--target",
                    &self.target_arch,
                    "--nocapture",
                    "-o",
                ])
                .arg(&installer_path_build),
        )?;

        cmd_spawn_wait(Command::new("powershell").args([
            "-Command",
            "$ErrorActionPreference = 'Stop';",
            "Compress-Archive",
            "-Force",
            "-Path",
            &installer_path_build.to_string_lossy(),
            "-DestinationPath",
            &zip_path_dist.to_string_lossy(),
        ]))?;

        self.write_sha256(
            &zip_path_dist,
            self.dist_dir.join(format!("{zip_name}-sha256.txt")),
        )
    }

    /// Compress web artifacts (.tar.gz).
    fn compress_web_artifacts(&self) -> anyhow::Result<()> {
        println!("compressing web artifacts...");

        let build_dir = self.dist_dir.join("web");
        self.tar_gz(
            format!(
                "{}-{}-{}.tar.gz",
                self.bin_name, self.version, self.target_arch
            ),
            &build_dir,
            ["."],
        )?;

        remove_dir_all(&build_dir)
    }
}

/// Helper function to `copy` a file and report contextual errors.
fn copy(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> anyhow::Result<u64> {
    let src = src.as_ref();
    let dst = dst.as_ref();

    println!("copying: {src:?} to {dst:?}");

    fs::copy(src, dst).with_context(|| format!("failed to copy {src:?} to {dst:?}"))
}

/// Helper function to `rename` a file and report contextual errors.
fn rename(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> anyhow::Result<()> {
    let src = src.as_ref();
    let dst = dst.as_ref();

    println!("renaming: {src:?} to {dst:?}");

    fs::rename(src, dst).with_context(|| format!("failed to rename {src:?} to {dst:?}"))
}

/// Helper function to `create_dir_all` a directory and report contextual errors.
fn create_dir_all(dir: impl AsRef<Path>) -> anyhow::Result<()> {
    let dir = dir.as_ref();

    println!("creating dir: {dir:?}");

    fs::create_dir_all(dir).with_context(|| format!("failed to create {dir:?}"))
}

/// Helper function to `remove_dir_all` a directory and report contextual errors.
fn remove_dir_all(dir: impl AsRef<Path>) -> anyhow::Result<()> {
    let dir = dir.as_ref();

    println!("removing dir: {dir:?}");

    fs::remove_dir_all(dir).with_context(|| format!("failed to remove {dir:?}"))
}

/// Helper function to `write` to a file and report contextual errors.
fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> anyhow::Result<()> {
    let path = path.as_ref();

    println!("writing to path: {path:?}");

    let contents = contents.as_ref();
    fs::write(path, contents).with_context(|| format!("failed to write to {path:?}"))
}

/// Helper function to `read_to_string` and report contextual errors.
#[cfg(target_os = "macos")]
fn read_to_string(path: impl AsRef<Path>) -> anyhow::Result<String> {
    let path = path.as_ref();

    println!("reading to string: {path:?}");

    fs::read_to_string(path).with_context(|| format!("failed to read {path:?}"))
}

/// Helper function to `symlink` and report contextual errors.
#[cfg(target_os = "macos")]
fn symlink(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> anyhow::Result<()> {
    use std::os::unix::fs::symlink;

    let src = src.as_ref();
    let dst = dst.as_ref();

    println!("symlinking: {src:?} to {dst:?}");

    symlink(src, dst).with_context(|| format!("failed to symlink {src:?} to {dst:?}"))
}

/// Helper function to `spawn` [`Command`] and `wait` while reporting contextual errors.
fn cmd_spawn_wait(cmd: &mut Command) -> anyhow::Result<ExitStatus> {
    println!("running: {cmd:?}");

    cmd.spawn()
        .with_context(|| format!("failed to spawn {cmd:?}"))?
        .wait()
        .with_context(|| format!("failed to run {cmd:?}"))
}

/// Helper function to run [`Command`] with `output` while reporting contextual errors.
fn cmd_output(cmd: &mut Command) -> anyhow::Result<Output> {
    println!("running: {cmd:?}");

    cmd.output()
        .with_context(|| format!("failed to run {cmd:?}"))
}

/// Helper function to run [`Command`] with `status` while reporting contextual errors.
#[cfg(target_os = "macos")]
fn cmd_status(cmd: &mut Command) -> anyhow::Result<ExitStatus> {
    println!("running: {cmd:?}");

    cmd.status()
        .with_context(|| format!("failed to run {cmd:?}"))
}
