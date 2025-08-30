#!/usr/bin/bash
cargo build --release
dir=~/.local/share/pop-launcher/plugins/cliphist/
mkdir -p $dir
cp target/release/cliphist $dir/
cp plugin.ron $dir/
