{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  nativeBuildInputs = with pkgs; [
    pkg-config
    cargo
    rustc
    # ⚠ CLIPPY DOIT VENIR D'ICI, JAMAIS DE LA MACHINE (ajouté le 19 août 2026).
    #
    # Sans cette ligne, `nix-shell shell.nix --run "cargo clippy"` prenait le `clippy-driver` du
    # système à côté du `rustc` du nix store. Les deux versions diffèrent, et la compilation
    # s'arrêtait sur un `E0514 : found crate compiled by an incompatible version of rustc` — DANS
    # UNE DÉPENDANCE (`thiserror`), ce qui ressemble à s'y méprendre à un cache abîmé.
    #
    # Conséquence : la barre « zéro warning » que ce projet se fixe était **invérifiable sur
    # AegisEngine, sans que rien ne le signale**. Exactement le même défaut que le cœur
    # `web3game`, corrigé chez lui le 9 août 2026 — et où il avait révélé un mécanisme mort et un
    # commentaire faux. Un outil qui refuse de démarrer se remarque ; un outil qu'on croit avoir
    # lancé, non.
    clippy
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
