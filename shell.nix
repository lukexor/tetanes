with (import <nixpkgs> {
  overlays = [
    (import (fetchTarball "https://github.com/oxalica/rust-overlay/archive/master.tar.gz"))
  ];
}); mkShell {
  buildInputs = [
    alsa-lib
    openssl
    pkg-config
    (rust-bin.selectLatestNightlyWith (toolchain: toolchain.default.override {
      extensions = [
        "rust-analyzer"
        "rust-src" # for rust-analyzer
      ];
      targets = ["wasm32-unknown-unknown"];
    }))
    trunk
    udev
  ];

  LD_LIBRARY_PATH = "${with pkgs; lib.makeLibraryPath [
      wayland
      libxkbcommon
      xorg.libX11
      xorg.libXcursor
      xorg.libXrandr
      xorg.libXi
      vulkan-loader
  ]}";
}
