use cfg_if::cfg_if;
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug)]
#[must_use]
struct Build {
    #[cfg(target_os = "macos")]
    version: &'static str,
    bin_name: &'static str,
    #[cfg(target_os = "macos")]
    app_name: &'static str,
    target_arch: &'static str,
    cargo_target_dir: PathBuf,
    dist_dir: PathBuf,
}

fn main() -> io::Result<()> {
    let build = Build::new()?;

    if env::args().nth(1).as_deref() == Some("web") {
        build.make("build-web")?;
        build.compress_web_artifacts()?;
    }

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

    Ok(())
}

impl Build {
    fn new() -> io::Result<Self> {
        let dist_dir = PathBuf::from("dist");

        let _ = fs::remove_dir_all(&dist_dir); // ignore if not found
        fs::create_dir_all(&dist_dir)?;

        Ok(Build {
            #[cfg(target_os = "macos")]
            version: env!("CARGO_PKG_VERSION"),
            bin_name: env!("CARGO_PKG_NAME"),
            #[cfg(target_os = "macos")]
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
            cargo_target_dir: PathBuf::from(
                env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string()),
            ),
            dist_dir: PathBuf::from("dist"),
        })
    }

    fn bin_path(&self) -> PathBuf {
        self.cargo_target_dir.join("dist").join(self.bin_name)
    }

    /// Run `cargo make` to build binary.
    ///
    /// Note: Wix on Windows bakes in the build step
    fn make(&self, cmd: &'static str) -> io::Result<()> {
        // TODO: disable lto and make pgo build
        Command::new("cargo").args(["make", cmd]).spawn()?.wait()?;

        Ok(())
    }

    /// Create a dist directory for artifacts.
    fn create_build_dir(&self, dir: impl AsRef<Path>) -> io::Result<PathBuf> {
        let build_dir = self.cargo_target_dir.join(dir);

        println!("creating build directory: {build_dir:?}");

        let _ = fs::remove_dir_all(&build_dir); // ignore if not found
        fs::create_dir_all(&build_dir)?;

        Ok(build_dir)
    }

    /// Write out a SHA256 checksum for a file.
    fn write_sha256(&self, file: impl AsRef<Path>, output: impl AsRef<Path>) -> io::Result<()> {
        let file = file.as_ref();
        let output = output.as_ref();

        println!("writing sha256 for {file:?}");

        let shasum = {
            cfg_if! {
                if #[cfg(target_os = "windows")] {
                    Command::new("powershell")
                        .arg("-Command")
                        .arg(format!("Get-FileHash -Algorithm SHA256 {} | select-object -ExpandProperty Hash", file.display()))
                        .output()?
                } else {
                    Command::new("shasum")
                        .current_dir(file.parent().expect("parent directory"))
                        .args(["-a", "256"])
                        .arg(file.file_name().expect("filename"))
                        .output()?
                }
            }
        };
        let sha256 = std::str::from_utf8(&shasum.stdout)
            .expect("valid stdout")
            .trim()
            .to_owned();

        println!("sha256: {sha256}");

        fs::write(output, shasum.stdout)?;

        Ok(())
    }

    fn tar_gz(
        &self,
        tgz_name: impl AsRef<str>,
        directory: impl AsRef<Path>,
        files: impl IntoIterator<Item = impl AsRef<Path>>,
    ) -> io::Result<()> {
        println!("creating tarball...");

        let tgz_name = tgz_name.as_ref();
        let mut cmd = Command::new("tar");
        cmd.arg("-czvf")
            .arg(self.dist_dir.join(tgz_name))
            .arg(format!("--directory={}", directory.as_ref().display()));
        for file in files {
            cmd.arg(file.as_ref());
        }
        cmd.spawn()?.wait()?;
        let tgz_sha_name = format!("{tgz_name}-sha256.txt");
        self.write_sha256(
            self.dist_dir.join(tgz_name),
            self.dist_dir.join(tgz_sha_name),
        )?;

        Ok(())
    }

    /// Create linux artifacts.
    #[cfg(target_os = "linux")]
    fn create_linux_artifacts(&self) -> io::Result<()> {
        println!("creating linux artifacts...");

        let build_dir = self.create_build_dir("linux")?;

        println!("creating tarball...");

        fs::copy("README.md", build_dir.join("README.md"))?;
        fs::copy("LICENSE-MIT", build_dir.join("LICENSE-MIT"))?;
        fs::copy("LICENSE-APACHE", build_dir.join("LICENSE-APACHE"))?;
        fs::copy(self.bin_path(), build_dir.join(self.bin_name))?;

        self.tar_gz(
            format!(
                "{}_{}-unknown-linux-gnu.tar.gz",
                self.bin_name, self.target_arch
            ),
            &build_dir,
            ["."],
        )?;

        println!("creating deb...");

        // NOTE: 1- is the deb revision number
        let deb_name = format!("{}_1-amd64.deb", self.bin_name);
        Command::new("cargo")
            .args(["deb", "-p", "tetanes", "-o"])
            .arg(self.dist_dir.join(&deb_name))
            .spawn()?
            .wait()?;
        let deb_sha_name = format!("{deb_name}-sha256.txt");
        self.write_sha256(
            self.dist_dir.join(&deb_name),
            self.dist_dir.join(deb_sha_name),
        )?;

        println!("creating AppImage...");

        let app_dir = build_dir.join("AppDir");

        Command::new(format!(
            "vendored/linuxdeploy-{}.AppImage",
            self.target_arch
        ))
        .arg("-e")
        .arg(self.bin_path())
        .arg("-i")
        .arg("assets/linux/icon.png")
        .arg("-d")
        .arg("assets/linux/TetaNES.desktop")
        .arg("--appdir")
        .arg(&app_dir)
        .arg("--output")
        .arg("appimage")
        .spawn()?
        .wait()?;

        let app_image_name = format!("{}_{}.AppImage", self.bin_name, self.target_arch);
        fs::rename(&app_image_name, self.dist_dir.join(&app_image_name))?;
        let app_image_sha_name = format!("{app_image_name}-sha256.txt");
        self.write_sha256(
            self.dist_dir.join(&app_image_name),
            self.dist_dir.join(app_image_sha_name),
        )?;

        Ok(())
    }

    /// Create macOS app.
    #[cfg(target_os = "macos")]
    fn create_macos_app(&self) -> io::Result<()> {
        use std::os::unix::fs::symlink;

        println!("creating macos app...");

        let artifact_name = format!("{}_{}", self.bin_name, self.target_arch);
        let volume = PathBuf::from("/Volumes").join(&artifact_name);
        let dmg_name = format!("{artifact_name}-Uncompressed.dmg");
        let dmg_name_compressed = format!("{artifact_name}.dmg");

        println!("creating dmg volume: {dmg_name_compressed}");

        let build_dir = self.create_build_dir("macos")?;

        let _ = Command::new("hdiutil").arg("detach").arg(&volume).status();
        Command::new("hdiutil")
            .args(["create", "-size", "50m", "-volname"])
            .arg(&artifact_name)
            .arg(build_dir.join(&dmg_name))
            .spawn()?
            .wait()?;
        Command::new("hdiutil")
            .arg("attach")
            .arg(build_dir.join(&dmg_name))
            .spawn()?
            .wait()?;

        println!("creating directories: {volume:?}");

        let app_dir = volume.join(format!("{}.app", self.app_name));
        fs::create_dir_all(app_dir.join("Contents/MacOS"))?;
        fs::create_dir_all(app_dir.join("Contents/Resources"))?;
        fs::create_dir_all(volume.join(".Picture"))?;

        println!("updating Info.plist version: {}", self.version);

        let mut info_plist = fs::read_to_string("assets/macos/Info.plist")?;
        info_plist = info_plist.replace("%VERSION%", self.version);
        fs::write(app_dir.join("Contents/Info.plist"), info_plist)?;

        println!("copying assets...");

        // TODO: maybe include readme/license?
        fs::copy(
            "assets/macos/Icon.icns",
            app_dir.join("Contents/Resources/Icon.icns"),
        )?;
        fs::copy(
            "assets/macos/background.png",
            volume.join(".Picture/background.png"),
        )?;
        fs::copy(
            self.bin_path(),
            app_dir.join("Contents/MacOS").join(self.bin_name),
        )?;

        println!("creating /Applications symlink...");
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
        Command::new("osascript")
            .arg("-e")
            .arg(&configure_bundle_script)
            .spawn()?
            .wait()?;

        println!("signing code...");
        Command::new("codesign")
            .args(["--force", "--sign", "-"])
            .arg(app_dir.join("Contents/MacOS").join(self.bin_name))
            .spawn()?
            .wait()?;
        // TODO: fix
        // ensure spctl --assess --type execute "${VOLUME}/${APP_NAME}.app"
        Command::new("codesign")
            .args(["--verify", "--strict", "--verbose=2"])
            .arg(app_dir.join("Contents/MacOS").join(self.bin_name))
            .spawn()?
            .wait()?;

        self.tar_gz(
            format!("{}_{}-apple-darwin.tar.gz", self.bin_name, self.target_arch),
            &volume,
            [&format!("{}.app", self.app_name)],
        )?;

        println!("compressing dmg...");

        Command::new("hdiutil")
            .arg("detach")
            .arg(&volume)
            .spawn()?
            .wait()?;
        Command::new("hdiutil")
            .args(["convert", "-format", "UDBZ", "-o"])
            .arg(build_dir.join(&dmg_name_compressed))
            .arg(build_dir.join(&dmg_name))
            .spawn()?
            .wait()?;

        println!("writing artifacts...");

        fs::copy(
            build_dir.join(&dmg_name_compressed),
            self.dist_dir.join(&dmg_name_compressed),
        )?;
        let dmg_sha_name = format!("{artifact_name}-sha256.txt");
        self.write_sha256(
            self.dist_dir.join(&dmg_name_compressed),
            self.dist_dir.join(dmg_sha_name),
        )?;

        println!("cleaning up...");

        fs::remove_file(build_dir.join(&dmg_name))?;

        Ok(())
    }

    /// Create Windows installer.
    #[cfg(target_os = "windows")]
    fn create_windows_installer(&self) -> io::Result<()> {
        println!("creating windows installer...");

        let build_dir = self.create_build_dir("wix")?;

        let installer_name = format!("{}_{}-pc-windows-msvc.msi", self.bin_name, self.target_arch);

        println!("building installer...");

        Command::new("cargo")
            .args(["wix", "-p", "tetanes", "--nocapture"])
            .spawn()?
            .wait()?;

        println!("writing artifacts...");

        fs::copy(
            build_dir.join(&installer_name),
            self.dist_dir.join(&installer_name),
        )?;
        let sha_name = format!("{installer_name}-sha256.txt");
        self.write_sha256(
            self.dist_dir.join(&installer_name),
            self.dist_dir.join(sha_name),
        )?;

        Ok(())
    }

    /// Compress web artifacts.
    fn compress_web_artifacts(&self) -> io::Result<()> {
        println!("compressing web artifacts...");

        self.tar_gz(
            format!("{}_-web.tar.gz", self.bin_name),
            self.dist_dir.join("web"),
            ["."],
        )?;

        println!("cleaning up...");

        fs::remove_dir_all(self.dist_dir.join("web"))?;

        Ok(())
    }
}
