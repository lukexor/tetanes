use cfg_if::cfg_if;
use std::{
    env,
    fs::{self, File},
    io::{self, Write},
    path::PathBuf,
    process::Command,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const BIN_NAME: &str = "tetanes";
const APP_NAME: &str = "TetaNES";

fn main() {
    // TODO: disable lto and make pgo build
    let mut handle = Command::new("cargo")
        .args(["make", "build-all"])
        .spawn()
        .expect("cargo build failed");
    let result = handle
        .wait()
        .expect("failed to read cargo build exit status");
    if !result.success() {
        panic!("cargo build failed");
    }

    cfg_if! {
        if #[cfg(target_os = "macos")] {
            create_macos_app().expect("failed to create macOS app");
        }
    }
}

#[cfg(target_os = "macos")]
fn create_macos_app() -> io::Result<()> {
    use std::os::unix::fs::symlink;

    let cargo_target_dir =
        PathBuf::from(env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string()));
    let build_dir = cargo_target_dir.join("macos");

    println!("creating build directory: {build_dir:?}");

    let _ = fs::remove_dir_all(&build_dir); // ignore if not found
    fs::create_dir_all(&build_dir)?;

    let volume_name = format!("{APP_NAME}-{VERSION}");
    let volume = PathBuf::from("/Volumes").join(&volume_name);
    let dmg_name = format!("{volume_name}-Uncompressed.dmg");
    let dmg_name_compressed = format!("{volume_name}.dmg");

    println!("creating dmg volume: {dmg_name}");

    let _ = Command::new("hdiutil").arg("detach").arg(&volume).status();
    Command::new("hdiutil")
        .args(["create", "-size", "50m", "-volname"])
        .arg(volume_name)
        .arg(build_dir.join(&dmg_name))
        .spawn()?
        .wait()?;
    Command::new("hdiutil")
        .arg("attach")
        .arg(build_dir.join(&dmg_name))
        .spawn()?
        .wait()?;

    println!("creating directories: {volume:?}");

    let app_dir = volume.join(format!("{APP_NAME}.app"));
    fs::create_dir_all(app_dir.join("Contents/MacOS"))?;
    fs::create_dir_all(app_dir.join("Contents/Resources"))?;
    fs::create_dir_all(volume.join(".Picture"))?;

    println!("updating Info.plist version: {VERSION:?}");

    let output = Command::new("sed")
        .arg("-e")
        .arg(format!("s/%VERSION%/{VERSION}/"))
        .arg("assets/macos/Info.plist")
        .output()?;
    File::create(app_dir.join("Contents/Info.plist"))?.write_all(&output.stdout)?;

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
        cargo_target_dir.join("dist").join(BIN_NAME),
        app_dir.join("Contents/MacOS").join(BIN_NAME),
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
                    set position of item "{APP_NAME}.app" to {{150, 300}}
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
        .arg(app_dir.join("Contents/MacOS").join(BIN_NAME))
        .spawn()?
        .wait()?;
    // TODO: fix
    // ensure spctl --assess --type execute "${VOLUME}/${APP_NAME}.app"
    Command::new("codesign")
        .args(["--verify", "--strict", "--verbose=2"])
        .arg(app_dir.join("Contents/MacOS").join(BIN_NAME))
        .spawn()?
        .wait()?;

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

    let shasum = Command::new("shasum")
        .args(["-a", "256"])
        .arg(build_dir.join(&dmg_name_compressed))
        .output()
        .map(|output| {
            std::str::from_utf8(&output.stdout)
                .expect("valid stdout")
                .trim()
                .to_owned()
        })?;
    println!("sha256: {shasum}");

    println!("cleaning up...");
    fs::remove_file(build_dir.join(&dmg_name))?;

    Ok(())
}
