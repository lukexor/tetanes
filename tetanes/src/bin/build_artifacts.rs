use anyhow::Context;
use cfg_if::cfg_if;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Output},
};

/// Build context with required variables and platform targets.
#[derive(Debug)]
#[must_use]
struct Build {
    #[cfg(target_os = "macos")]
    version: &'static str,
    bin_name: &'static str,
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    app_name: &'static str,
    target_arch: &'static str,
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    cargo_target_dir: PathBuf,
    dist_dir: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let build = Build::new()?;

    println!("building artifacts: {build:?}...");

    if env::args().nth(1).as_deref() == Some("web") {
        build.make("build-web")?;
        build.compress_web_artifacts()?;
    } else {
        cfg_if! {
            if #[cfg(target_os = "linux")] {
                build.make("build")?;
                build.create_linux_artifacts()?;
            } else if #[cfg(target_os = "macos")] {
                build.make("build")?;
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
    fn new() -> anyhow::Result<Self> {
        let dist_dir = PathBuf::from("dist");

        let _ = remove_dir_all(&dist_dir); // ignore if not found
        create_dir_all(&dist_dir)?;

        Ok(Build {
            #[cfg(target_os = "macos")]
            version: env!("CARGO_PKG_VERSION"),
            bin_name: env!("CARGO_PKG_NAME"),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            app_name: "TetaNES",
            target_arch: if cfg!(target_arch = "x86_64") {
                "x86_64"
            } else if cfg!(target_arch = "aarch64") {
                "aarch64"
            } else if cfg!(target_arch = "wasm32") {
                "wasm32"
            } else {
                panic!("unsupported target arch");
            },
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            cargo_target_dir: PathBuf::from(
                env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string()),
            ),
            dist_dir: PathBuf::from("dist"),
        })
    }

    /// Path to the binary executable built with `cargo`. Windows and wasm32 target build steps
    /// already include the binary.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn bin_path(&self) -> PathBuf {
        self.cargo_target_dir.join("dist").join(self.bin_name)
    }

    /// Run `cargo make` to build binary.
    ///
    /// Note: Wix on Windows bakes in the build step
    fn make(&self, cmd: &'static str) -> anyhow::Result<ExitStatus> {
        // TODO: disable lto and make pgo build
        cmd_spawn_wait(Command::new("cargo").args(["make", cmd]))
    }

    /// Create a dist directory for artifacts.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
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
                        .arg("-Command")
                        .arg(format!("Get-FileHash -Algorithm SHA256 {} | select-object -ExpandProperty Hash", file.display())))?
                } else {
                    cmd_output(Command::new("shasum")
                        .current_dir(file.parent().expect("parent directory"))
                        .args(["-a", "256"])
                        .arg(file.file_name().expect("filename")))?
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

        copy("README.md", build_dir.join("README.md"))?;
        copy("LICENSE-MIT", build_dir.join("LICENSE-MIT"))?;
        copy("LICENSE-APACHE", build_dir.join("LICENSE-APACHE"))?;

        let build_bin_path = build_dir.join(self.bin_name);
        copy(self.bin_path(), &build_bin_path)?;

        self.tar_gz(
            format!(
                "{}-{}-unknown-linux-gnu.tar.gz",
                self.bin_name, self.target_arch
            ),
            &build_dir,
            ["."],
        )?;

        // NOTE: 1- is the deb revision number
        let deb_name = format!("{}-1-amd64.deb", self.bin_name);
        let deb_path_dist = self.dist_dir.join(&deb_name);
        cmd_spawn_wait(
            Command::new("cargo")
                .args(["deb", "-p", "tetanes", "-o"])
                .arg(&deb_path_dist),
        )?;
        self.write_sha256(
            &deb_path_dist,
            self.dist_dir.join(format!("{deb_name}-sha256.txt")),
        )?;

        let linuxdeploy_cmd = format!("vendored/linuxdeploy-{}.AppImage", self.target_arch);
        let app_dir = build_dir.join("AppDir");
        let desktop_name = format!("assets/linux/{}.desktop", self.bin_name);
        cmd_spawn_wait(
            Command::new(&linuxdeploy_cmd)
                .arg("-e")
                .arg(self.bin_path())
                .args(["-i", "assets/linux/icon.png", "-d"])
                .arg(&desktop_name)
                .arg("--appdir")
                .arg(&app_dir)
                .args(["--output", "appimage"]),
        )?;

        // NOTE: AppImage name is derived from tetanes.desktop
        let app_image_name = format!("{}-{}.AppImage", self.app_name, self.target_arch);
        let app_image_path = PathBuf::from(&app_image_name);
        // Rename to lowercase
        let app_image_path_dist = self
            .dist_dir
            .join(format!("{}-{}.AppImage", self.bin_name, self.target_arch));
        rename(&app_image_path, &app_image_path_dist)?;
        self.write_sha256(
            &app_image_path_dist,
            self.dist_dir.join(format!("{app_image_name}-sha256.txt")),
        )
    }

    /// Create macOS artifacts (.app in a .tar.gz and separate .dmg).
    #[cfg(target_os = "macos")]
    fn create_macos_app(&self) -> anyhow::Result<()> {
        println!("creating macos app...");

        let build_dir = self.create_build_dir("macos")?;

        let artifact_name = format!("{}-{}", self.bin_name, self.target_arch);
        let volume = PathBuf::from("/Volumes").join(&artifact_name);
        let dmg_name = format!("{artifact_name}-uncompressed.dmg");
        let dmg_path = build_dir.join(&dmg_name);
        let dmg_name_compressed = format!("{artifact_name}.dmg");
        let dmg_path_compressed = build_dir.join(&dmg_name_compressed);
        let dmg_path_dist = self.dist_dir.join(&dmg_name_compressed);

        if let Err(err) = cmd_status(Command::new("hdiutil").arg("detach").arg(&volume)) {
            eprintln!("failed to detach volume: {err:?}");
        }
        cmd_spawn_wait(
            Command::new("hdiutil")
                .args(["create", "-size", "50m", "-volname"])
                .arg(&artifact_name)
                .arg(&dmg_path),
        )?;
        cmd_spawn_wait(Command::new("hdiutil").arg("attach").arg(&dmg_path))?;

        let app_dir = volume.join(format!("{}.app", self.app_name));
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
        copy(
            self.bin_path(),
            app_dir.join("Contents/MacOS").join(self.bin_name),
        )?;

        symlink("/Applications", volume.join("Applications"))?;

        println!("configuring app bundle window...");

        let configure_bundle_script = format!(
            r#"
        tell application "Finder"
            set f to POSIX file ("{volume}" as string) as alias
            tell folder f
                open
                    tell container window
                        set toolbar visible to false
                        set statusbar visible to false
                        set current view to icon view
                        delay 1 -- sync
                        set the bounds to {{0, 0, 720, 524}}
                    end tell
                    delay 1 -- sync
                    set icon size of the icon view options of container window to 120
                    set arrangement of the icon view options of container window to not arranged
                    set position of item ".Picture" to {{800, 320}}
                    set position of item ".fseventsd" to {{800, 320}}
                    set position of item "{app_name}.app" to {{150, 300}}
                close
                set position of item "Applications" to {{425, 300}}
                open
                    set background picture of the icon view options of container window to file "background.png" of folder ".Picture"
                    set the bounds of the container window to {{0, 0, 600, 524}}
                    update without registering applications
                    delay 1 -- sync
                close
            end tell
            delay 1 -- sync
        end tell
    "#,
            app_name = self.app_name,
            volume = volume.display()
        );
        cmd_spawn_wait(
            Command::new("osascript")
                .arg("-e")
                .arg(&configure_bundle_script),
        )?;

        let bin_path = app_dir.join("Contents/MacOS").join(self.bin_name);
        cmd_spawn_wait(
            Command::new("codesign")
                .args(["--force", "--sign", "-"])
                .arg(&bin_path),
        )?;
        // TODO: fix
        // ensure spctl --assess --type execute "${VOLUME}/${APP_NAME}.app"
        cmd_spawn_wait(
            Command::new("codesign")
                .args(["--verify", "--strict", "--verbose=2"])
                .arg(&bin_path),
        )?;

        self.tar_gz(
            format!("{}-{}-apple-darwin.tar.gz", self.bin_name, self.target_arch),
            &volume,
            [&format!("{}.app", self.app_name)],
        )?;

        cmd_spawn_wait(Command::new("hdiutil").arg("detach").arg(&volume))?;
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

        let installer_name = format!("{}-{}.msi", self.bin_name, self.target_arch);
        let installer_path_dist = self.dist_dir.join(&installer_name);

        cmd_spawn_wait(
            Command::new("cargo")
                .args(["wix", "-p", "tetanes", "--nocapture", "-o"])
                .arg(&installer_path_dist),
        )?;

        // TODO: maybe zip installer?
        self.write_sha256(
            &installer_path_dist,
            self.dist_dir.join(format!("{installer_name}-sha256.txt")),
        )
    }

    /// Compress web artifacts (.tar.gz).
    fn compress_web_artifacts(&self) -> anyhow::Result<()> {
        println!("compressing web artifacts...");

        self.tar_gz(
            format!("{}-web.tar.gz", self.bin_name),
            self.dist_dir.join("web"),
            ["."],
        )?;

        remove_dir_all(self.dist_dir.join("web"))
    }
}

/// Helper function to `copy` a file and report contextual errors.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn copy(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> anyhow::Result<u64> {
    let src = src.as_ref();
    let dst = dst.as_ref();

    println!("copying: {src:?} to {dst:?}");

    fs::copy(src, dst).with_context(|| format!("failed to copy {src:?} to {dst:?}"))
}

/// Helper function to `rename` a file and report contextual errors.
#[cfg(any(target_os = "linux", target_os = "macos"))]
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
