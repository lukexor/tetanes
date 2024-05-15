BIN_NAME="tetanes"
APP_NAME="TetaNES"
VERSION="0.10.0"
BUILD_DIR="tetanes/dist"

build() {
  need_cmd cargo

  ensure cargo make build-all
  # TODO: disable lto and make pgo build
}

make_artifacts() {
    need_cmd cp
    need_cmd mkdir

    if linux; then
        echo "todo: linux artifacts"
    elif macos; then
        APP_PATH="${APP_NAME}.app"
        VOL_NAME="${APP_NAME}-${VERSION}"
        DMG_NAME="${VOL_NAME}Uncompressed.dmg"
        DMG_NAME_COMPRESSED="${VOL_NAME}.dmg"

        ignore rm -rf "${BUILD_DIR}"
        ensure mkdir -p "${BUILD_DIR}"

        ensure hdiutil create -size 50m -volname "${VOL_NAME}" "${BUILD_DIR}/${DMG_NAME}"
        ensure hdiutil attach "${BUILD_DIR}/${DMG_NAME}"

        DEVS=$(hdiutil attach "${BUILD_DIR}/${DMG_NAME}" | grep ${APP_NAME} | cut -f 1)
        DEV=$(echo "$DEVS" | cut -f 1 -d ' ')
        VOLUME=$(mount |grep "${DEV}" | cut -f 3 -d ' ')

        MOUNTED=1
        unmount() {
            if [ "$MOUNTED" -eq 1 ]; then
                for i in $(seq 1 3); do
                    if hdiutil detach "$DEV"; then
                        break
                    fi
                    if [ "$i" -eq 3 ]; then
                        echo "Failed to unmount the disk image; exiting after 3 retries."
                        exit 1
                    fi
                    echo "Failed to unmount the disk image; retrying in 1s"
                    sleep 1
                done
                MOUNTED=0
            fi
        }
        trap unmount EXIT

        ensure mkdir -p "${VOLUME}/${APP_PATH}/Contents/MacOS"
        ensure mkdir -p "${VOLUME}/${APP_PATH}/Contents/Resources"
        ensure mkdir -p "${VOLUME}/.background"

        ensure cp -Ra "${CARGO_TARGET_DIR}/dist/${BIN_NAME}" "${VOLUME}/${APP_PATH}/Contents/MacOS"
        ensure cp assets/Info.plist "${VOLUME}/${APP_PATH}/Contents"
        ensure cp assets/logo.icns "${VOLUME}/${APP_PATH}/Contents/Resources"
        ensure cp static/tetanes.png "${VOLUME}/.background/tetanes.png"

        ensure ln -s /Applications "${VOLUME}/Applications"
        set_bundle_display_options "${VOLUME}"

        ensure codesign --force --sign - "${VOLUME}/${APP_NAME}.app/Contents/MacOS/${BIN_NAME}"
        # TODO: fix
        # ensure spctl --assess --type execute "${VOLUME}/${APP_NAME}.app"
        ensure codesign --verify --strict --verbose=2 "${VOLUME}/${APP_NAME}.app/Contents/MacOS/${BIN_NAME}"

        unmount
        hdiutil convert "${BUILD_DIR}/${DMG_NAME}" -format UDBZ -o "${BUILD_DIR}/${DMG_NAME_COMPRESSED}"

        shasum -a 256 "${BUILD_DIR}/${DMG_NAME_COMPRESSED}"

        ignore rm "${BUILD_DIR}/${DMG_NAME}"
    fi

    ensure ls "${BUILD_DIR}"
}

upload() {
    echo "todo: upload artifacts to github release"
    echo "todo: update homebrew-formulae/Formula/tetanes version/sha"
    echo "todo: commit web artifacts to lukeworks"
}

# Returns true of OS is Linux
linux() {
  [[ "$(uname -s)" == Linux* ]]
}

# Returns true of OS is macOS
macos() {
  [[ "$(uname -s)" == Darwin* ]]
}

need_cmd() {
    if ! check_cmd "$1"
    then err "need '$1' (command not found)"
    fi
}

check_cmd() {
    command -v "$1" > /dev/null 2>&1
    return $?
}

# Run a command that should never fail. If the command fails execution
# will immediately terminate with an error showing the failing
# command.
ensure() {
    if ! "$@"; then err "command failed: $*"; fi
}

err() {
    if [ "0" = "$PRINT_QUIET" ]; then
        local red
        local reset
        red=$(tput setaf 1 2>/dev/null || echo '')
        reset=$(tput sgr0 2>/dev/null || echo '')
        say "${red}ERROR${reset}: $1" >&2
    fi
    exit 1
}

# This is just for indicating that commands' results are being
# intentionally ignored. Usually, because it's being executed
# as part of error handling.
ignore() {
    "$@"
}

function set_bundle_display_options() {
	osascript <<-EOF
    tell application "Finder"
        set f to POSIX file ("${1}" as string) as alias
        tell folder f
            open
            tell container window
                set toolbar visible to false
                set statusbar visible to false
                set current view to icon view
                delay 1 -- sync
                set the bounds to {0, 0, 720, 524}
            end tell
            delay 1 -- sync
            set icon size of the icon view options of container window to 120
            set arrangement of the icon view options of container window to not arranged
            set position of item ".background" to {800, 320}
            set position of item ".fseventsd" to {800, 320}
            set position of item "${APP_NAME}.app" to {150, 300}
            close
            set position of item "Applications" to {425, 300}
            open
            set background picture of the icon view options of container window to file "tetanes.png" of folder ".background"
            set the bounds of the container window to {0, 0, 600, 524}
            update without registering applications
            delay 5 -- sync
            close
        end tell
        delay 5 -- sync
    end tell
EOF
}

build
make_artifacts
upload
