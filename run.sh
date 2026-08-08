#!/usr/bin/env bash
# Script de lancement automatique pour AegisEngine sous NixOS / Linux

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"

if command -v nix-shell &> /dev/null; then
    nix-shell "$DIR/shell.nix" --run "cargo run --manifest-path $DIR/Cargo.toml -- $@"
else
    cargo run --manifest-path "$DIR/Cargo.toml" -- "$@"
fi

