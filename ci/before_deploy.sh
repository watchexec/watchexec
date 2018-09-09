#!/usr/bin/env bash

# Build script shamelessly stolen from ripgrep :)

cargo build --target $TARGET --release

build_dir=$(mktemp -d 2>/dev/null || mktemp -d -t tmp)
out_dir=$(pwd)
name="${PROJECT_NAME}-${TRAVIS_TAG}-${TARGET}"
mkdir "$build_dir/$name"

cp target/$TARGET/release/watchexec "$build_dir/$name/"
cp {doc/watchexec.1,LICENSE} "$build_dir/$name/"

pushd $build_dir
tar cvf "$out_dir/$name.tar" *
popd
gzip -f9 "$name.tar"


if [[ "$TARGET" == "x86_64-unknown-linux-gnu" ]]; then
    mkdir -p "$build_dir/deb/$name"
    pushd "$build_dir/deb/$name"

    mkdir -p DEBIAN usr/bin usr/share/man/man1
    cp "../../$name/watchexec" usr/bin/
    cp "../../$name/watchexec.1" usr/share/man/man1/
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

