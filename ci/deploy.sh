#!/usr/bin/env bash

if [[ ! -z "$CARGO_CLIPPY" ]]; then
    echo Clippy says: I shan’t deploy
    exit 1
fi

### Vars

project="$PROJECT_NAME"
tag="$TRAVIS_TAG"
target="$TARGET"

[[ -z "$project" ]] && exit 21
[[ -z "$tag" ]] && exit 22
[[ -z "$target" ]] && exit 23

ext=""
windows=""
if [[ "$target" == *"windows"* ]]; then
    choco install 7zip
    ext=".exe"
    windows="1"
fi

build_dir=$(mktemp -d 2>/dev/null || mktemp -d -t tmp)
out_dir=$(pwd)
name="$project-$tag-$target"

### Build

cargo build --target $target --release --locked

### Decorate

mkdir "$build_dir/$name"
cp -v "target/$target/release/$project$ext" "$build_dir/$name/"
cp -v LICENSE "$build_dir/$name/"
cp -v README.md "$build_dir/$name/"
cp -v completions/zsh "$build_dir/$name/"
cp -v doc/watchexec.1 "$build_dir/$name/"
ls -shal "$build_dir/$name/"

### Strip

cd "$build_dir"
strip "$name/$project$ext"
ls -shal "$name/"

### Pack

if [[ -z "$windows" ]]; then
    tar cvf "$out_dir/$name.tar" "$name"
    cd "$out_dir"
    xz -f9 "$name.tar"
else
    7z a "$out_dir/$name.zip" "$name"
fi

### Debify

if [[ "$target" == "x86_64-unknown-linux-gnu" ]]; then
    mkdir -p "$build_dir/deb/$name"
    cd "$build_dir/deb/$name"

    mkdir -p DEBIAN usr/bin usr/share/man/man1 usr/share/zsh/site-functions
    cp "../../$name/watchexec" usr/bin/
    cp "../../$name/watchexec.1" usr/share/man/man1/
    cp "../../$name/zsh" usr/share/zsh/site-functions/_watchexec
    cat <<CONTROL > DEBIAN/control
Package: watchexec
Version: ${TRAVIS_TAG}
Architecture: amd64
Maintainer: Félix Saparelli <aur@passcod.name>
Installed-Size: $(du -d1 usr | tail -n1 | cut -d\t -f1)
Homepage: https://github.com/watchexec/watchexec
Description: Executes commands in response to file modifications.
 Software development often involves running the same commands over and over. Boring!
 Watchexec is a simple, standalone tool that watches a path and runs a command whenever it detects modifications.
CONTROL
    cd ..
    fakeroot dpkg -b "$name"
    mv "$name.deb" "$out_dir/"
    popd
fi

ls -shal "$out_dir/"
