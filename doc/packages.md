# Known packages of Watchexec

Note that only first-party packages are maintained here.
Anyone is welcome to create and maintain a packaging of
Watchexec for their platform/distribution and submit it
to their upstreams, and anyone may submit a PR to update
this list. To report issues with non-first-party packages
(outside of bugs that belong to Watchexec), contact the
relevant packager.

| Platform | Distributor | Package name | Status | Install command |
|:-|:-|:-:|:-:|-:|
| Linux | _n/a_ (deb) | [`watchexec-{version}-{platform}.deb`](https://github.com/watchexec/watchexec/releases) | first-party | `dpkg -i watchexec-*.deb` |
| Linux | _n/a_ (rpm) | [`watchexec-{version}-{platform}.rpm`](https://github.com/watchexec/watchexec/releases) | first-party | `dnf install watchexec-*.deb` |
| Linux | _n/a_ (tarball) | [`watchexec-{version}-{platform}.tar.xz`](https://github.com/watchexec/watchexec/releases) | first-party | `tar xf watchexec-*.tar.xz` |
| Linux | Alpine | [`watchexec`](https://pkgs.alpinelinux.org/packages?name=watchexec) | official | `apk add watchexec` |
| Linux | ALT Sisyphus | [`watchexec`](https://packages.altlinux.org/en/sisyphus/srpms/watchexec/) | official | `apt-get install watchexec` |
| Linux | [APT repo](https://apt.cli.rs) (Debian & Ubuntu) | [`watchexec-cli`](https://apt.cli.rs) | community | `apt install watchexec-cli` |
| Linux | Arch | [`watchexec`](https://archlinux.org/packages/extra/x86_64/watchexec/) | official | `pacman -S watchexec` |
| Linux | Gentoo GURU | [`watchexec`](https://gpo.zugaina.org/Overlays/guru/app-misc/watchexec) | community | `emerge -av watchexec` |
| Linux | GNU Guix | [`watchexec`](https://packages.guix.gnu.org/packages/watchexec/) | outdated | `guix install watchexec` |
| Linux | LiGurOS | [`watchexec`](https://gitlab.com/liguros/liguros-repo/-/tree/stable/app-misc/watchexec) | official | `emerge -av watchexec` |
| Linux | Manjaro | [`watchexec`](https://software.manjaro.org/package/watchexec) | official | `pamac install watchexec` |
| Linux | Nix | [`watchexec`](https://search.nixos.org/packages?query=watchexec) | official | `nix-shell -p watchexec` |
| Linux | openSUSE | [`watchexec`](https://software.opensuse.org/package/watchexec) | official | `zypper install watchexec` |
| Linux | pacstall (Ubuntu) | [`watchexec-cli`](https://pacstall.dev/packages/watchexec-bin) | community | `pacstall -I watchexec-bin` |
| Linux | Parabola | [`watchexec`](https://www.parabola.nu/packages/?q=watchexec) | official | `pacman -S watchexec` |
| Linux | Solus | [`watchexec`](https://github.com/getsolus/packages/blob/main/packages/w/watchexec/package.yml) | official | `eopkg install watchexec` |
| Linux | Termux (Android) | [`watchexec`](https://github.com/termux/termux-packages/blob/master/packages/watchexec/build.sh) | official | `pkg install watchexec` |
| Linux | Void | [`watchexec`](https://github.com/void-linux/void-packages/tree/master/srcpkgs/watchexec) | official | `xbps-install watchexec` |
| MacOS | _n/a_ (tarball) | [`watchexec-{version}-{platform}.tar.xz`](https://github.com/watchexec/watchexec/releases) | first-party | `tar xf watchexec-*.tar.xz` |
| MacOS | Homebrew | [`watchexec`](https://formulae.brew.sh/formula/watchexec) | official | `brew install watchexec` |
| MacOS | MacPorts | [`watchexec`](https://ports.macports.org/port/watchexec/summary/) | official | `port install watchexec` |
| Windows | _n/a_ (zip) | [`watchexec-{version}-{platform}.zip`](https://github.com/watchexec/watchexec/releases) | first-party | `Expand-Archive -Path watchexec-*.zip` |
| Windows | Baulk | [`watchexec`](https://github.com/baulk/bucket/blob/master/bucket/watchexec.json) | official | `baulk install watchexec` |
| Windows | Chocolatey | [`watchexec`](https://community.chocolatey.org/packages/watchexec) | community | `choco install watchexec` |
| Windows | MSYS2 mingw | [`mingw-w64-watchexec`](https://github.com/msys2/MINGW-packages/blob/master/mingw-w64-watchexec) | official | `pacman -S mingw-w64-x86_64-watchexec` |
| Windows | Scoop | [`watchexec`](https://github.com/ScoopInstaller/Main/blob/master/bucket/watchexec.json) | official | `scoop install watchexec` |
| _Any_ | Crates.io | [`watchexec-cli`](https://crates.io/crates/watchexec-cli) | first-party | `cargo install --locked watchexec-cli` |
| _Any_ | Binstall | [`watchexec-cli`](https://crates.io/crates/watchexec-cli) | first-party | `cargo binstall watchexec-cli` |
| _Any_ | Webi | [`watchexec`](https://webinstall.dev/watchexec/) | third-party | varies (see webpage) |

Legend:
- first-party: packaged and distributed by the Watchexec developers (in this repo)
- official: packaged and distributed by the official package team for the listed distribution
- community: packaged by a community member or organisation, outside of the official distribution
- third-party: a redistribution of another package (e.g. using the first-party tarballs via a non-first-party installer)
- outdated: an official or community packaging that is severely outdated (not just a couple releases out)
