{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  nativeBuildInputs = with pkgs; [
    pkg-config
    cargo
    rustc
  ];

  buildInputs = with pkgs; [
    libx11
    libxcursor
    libxrandr
    libxi
    libxrender
    libxkbcommon
    wayland
    vulkan-loader
  ];

  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (with pkgs; [
    libx11
    libxcursor
    libxrandr
    libxi
    libxrender
    libxkbcommon
    wayland
    vulkan-loader
  ]);
}
