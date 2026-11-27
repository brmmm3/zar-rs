# Packaging

Installers for `zarc` (CLI) and `zarg` (GUI). The Windows suite installer
also includes `zcopy` and `zarmount`. Output goes to `dist/`.

The installed `zarc` CLI and `zarg` GUI support custom archive-level key/value
metadata. For example, use `zarc meta backup.zar --set project=example` to
write a value and `zarc meta backup.zar` to list values; updates append a
metadata-only commit. See the [CLI guide](../zarlib-cli/README.md#append-remove-and-compact-commits).

Pushing a stable `x.y.z` tag on a `master` commit runs
[CI](../.github/workflows/ci.yml), followed by
[packaging](../.github/workflows/packaging.yml), v3 CI, and v3 packaging.
Each attaches its assets to the `x.y.z` release. CI also runs on
pull requests and supports manual dispatch; selecting a branch builds workflow
artifacts without attaching them to a release.

The release packaging workflow reuses binaries from its calling CI run.
Standalone packaging attempts to reuse binaries from the latest successful CI
run on `master` for the exact commit when those artifacts are still retained.
If no matching artifact is available, it builds from source. Flatpak
continues to build inside its Freedesktop SDK. Normal x86-64 builds explicitly
use the baseline CPU target for Rust and native C/C++ code where applicable;
static builds retain that target. Windows archives contain all four installer
executables. If a reused Windows archive lacks `zarmount`, packaging builds it
from the same source. Baseline and ARM package filenames retain their usual names.

The `x.y.z` release also includes x86-64-v3 packages built by
`.github/workflows/packaging-v3.yml`. It reuses the matching v3 release
binaries, including Windows `zcopy` and `zarmount`, and sets v3 compiler flags
inside the Flatpak SDK. Older v3 binary archives without required installer
executables are supported by building them during packaging. Package filenames carry a `-v3` suffix; their
internal package versions are unchanged. OS, system
library, and Flatpak runtime requirements still apply. The workflow runs after
the v3 release workflow or can be dispatched manually with its source tag.
The v1 and v3 packages use the same internal package version and identity, so they replace
one another rather than installing side by side. These workflow changes do
not rebuild assets already published in older releases.

## Updating package versions

From the repository root, use Python 3.11 or newer:

```sh
python3 scripts/update_version.py 0.17.0 --dry-run
python3 scripts/update_version.py 0.17.0
```

The script updates the Rust workspace and local dependency versions, Cargo and
Node.js lockfiles, Python, Java, NuGet, Scoop, Chocolatey, Inno Setup and Arch
package versions, and current release examples in the READMEs.
It preserves file line endings, third-party dependency versions, changelog history,
and existing versioned winget manifests. It does not create Git tags or publish.
Scoop and Chocolatey checksums are deliberately left unchanged. After the release
assets are published, update them before publishing or installing the packages:

```sh
python3 scripts/update_checksums.py 0.17.0 --dry-run
python3 scripts/update_checksums.py 0.17.0
```

The checksum updater fetches SHA-256 digests from GitHub and defaults to the Scoop
manifest version when no version is provided. Both scripts prepare and validate
all manifest edits before writing files. The Arch PKGBUILD's source revision
and checksum remain pinned and must be updated separately before building.

## Windows (Inno Setup)

```powershell
cargo build --release --bins
iscc /DAppVersion=0.17.0 packaging\windows\zar-rs.iss
```

Use `/DSourceDir=target\<triple>\release` when building with `--target`, and
`/DArch=arm64` for Windows on ARM. The installer can add `zarc` to the system
`PATH` along with `zcopy`, create Start menu and desktop shortcuts, and register Explorer verbs
for files, folders and `.zar` archives. Explorer passes multiple selections to
the create actions. Verbs for opening and extracting archives appear only for supported archive formats
(`.zar`, `.zip`, `.7z`, `.rar`, `.iso`, `.tar`, `.tgz`, `.gz`), **Mount Archive** appears only for `.zar` archives,
and **Convert to ZAR** converts selected archives into `.zar` archives beside the originals (appearing only for folders
and convertible archive formats: `.zip`, `.7z`, `.rar`, `.iso`, `.tar`, `.tgz`, `.gz`, and hidden for `.zar` and unsupported files).
**Extract to archive folder** extracts `foo.zar` into a sibling folder named `foo`.
The `.zar` file icon is generated from the PNG MIME icon.

### Windows Package Manager

Versioned winget manifests are in `packaging/windows/winget`. Install one
locally with, for example:

```powershell
winget install --manifest packaging\windows\winget\Brmmm3.Zcopy\0.11.0
```

Validate all manifest sets with `./packaging/windows/winget/validate.ps1`.

The `zarc` and `zarg` manifests install portable executables from the release
ZIPs; the portable `zarg` does not register Explorer actions or create a Start
menu shortcut. The 0.11.0 ZIPs do not contain `zcopy`, so its manifest uses
the machine-wide Inno installer, requires administrator approval, and installs
all three programs with `zarc` and `zcopy` on the system PATH. Uninstalling
that package removes the whole installed suite. Installing the portable
packages as well can result in duplicate executables on PATH. These manifests
are local files and are not available through the winget community source until
submitted and accepted. Update the version, URLs, and hashes for each release.

### Chocolatey

The `zar-rs` and `zar-rs-v3` packages install portable copies of `zarc`,
`zarg`, and `zcopy` with Chocolatey shims. Build and install the baseline
package locally, for example:

```powershell
Push-Location packaging\windows\chocolatey\zar-rs
choco pack zarg.nuspec --output-directory "$env:TEMP"
Pop-Location
choco install zar-rs --version 0.17.0 --source "$env:TEMP" -y
```

For the x86-64-v3 package, pack
`packaging\windows\chocolatey\zar-rs-v3\zarc.nuspec` and install `zar-rs-v3`.
These portable packages do not register Explorer integration. The Chocolatey
v3 package requires an x86-64-v3-compatible processor. Before downloading, it
checks that Windows reports AVX2 support and fails if support cannot be
confirmed. This check requires Windows 10 version 2004 or later and does not
check every x86-64-v3 feature; AVX2 alone does not guarantee compatibility.

### Scoop

Install a local manifest, for example:

```powershell
scoop install .\packaging\windows\scoop\zar-rs.json
# On a compatible x86-64-v3 system:
scoop install .\packaging\windows\scoop\zar-rs-v3.json
```

Both Scoop manifests expose `zarc`, `zarg`, and `zcopy` and create a `zarg`
Start menu shortcut. They do not register Explorer integration. The v3
manifest checks that Windows reports AVX2 support; if it cannot confirm support,
installation fails and the baseline package should be used instead. This check
requires Windows 10 version 2004 or later and does not check every x86-64-v3
feature, such as BMI2 and FMA. AVX2 alone does not guarantee v3 compatibility.
Scoop may select the v3 x64 build under emulation on Windows ARM; use the
baseline manifest for its native ARM64 build.

Avoid installing baseline and v3 packages together or combining package
managers, as they expose the same commands. The Scoop and Chocolatey packages
have not been published to their community repositories. Update versions,
URLs, and checksums for each release.

## Linux

### NixOS

The local Nix expression builds `zarc`, `zarg`, `zcopy`, `zarmcp`, and
`zarmount` from this checkout using its Cargo lockfile. It also installs
the `zarg` desktop entries, icons, and MIME definition. Use a recent Nixpkgs
with a Rust compiler capable of building the workspace. From the repository
root, build and inspect the package:

```sh
nix-build -E 'with import <nixpkgs> {}; callPackage ./packaging/linux/zar-rs.nix {}'
./result/bin/zarc --version
```

In a NixOS configuration, import the expression from a local checkout:

```nix
environment.systemPackages = [
  (pkgs.callPackage /path/to/zar-rs/packaging/linux/zar-rs.nix {})
];
```

The GUI needs a working X11 or Wayland desktop and an XDG desktop portal for
native file dialogs. `zarmount` needs access to `/dev/fuse` and the system
`fusermount3` helper; see the [mount guide](../zarmount/README.md). File-manager
context menus are not installed by this expression. The package is not
published to Nixpkgs.

### Arch Linux

The local PKGBUILD builds `zarc`, `zarg`, `zcopy`, `zarmcp`, and `zarmount`
from a pinned 0.16.0 development commit. It also installs the same desktop and
file-manager integration as the deb and RPM packages. On Arch Linux or Arch
Linux ARM, install `base-devel` and run:

```sh
cd packaging/linux/arch
makepkg -si
```

The package is not published to the AUR. `makepkg` downloads Rust dependencies
during `prepare()`; subsequent build steps use Cargo's frozen lockfile. The
source archive is downloaded in full, but its root `testdata` directory is
excluded when unpacking the build sources. For new releases, update `_commit`
to the corresponding source revision and run `updpkgsums`; the version updater
sets `pkgver`.
Thunar users should run `zarg-install-thunar-actions` once after installation.

### Other Linux packages

```sh
packaging/linux/build-packages.sh [deb|rpm|appimage|flatpak|all]
```

| Format   | Tool needed                         | Result                          |
|----------|-------------------------------------|---------------------------------|
| deb      | `dpkg-deb`                          | `zar-rs_<version>_<arch>.deb`   |
| rpm      | `rpmbuild`                          | `zar-rs-<version>-1.<arch>.rpm` |
| appimage | `appimagetool` (or `APPIMAGETOOL`)  | `ZAR-<version>-<arch>.AppImage` |
| flatpak  | `flatpak-builder`, `flatpak`        | `zar-rs-<version>-<arch>.flatpak` |

deb, rpm and AppImage are assembled from the release binaries
(`SKIP_BUILD=1` reuses existing ones; `BIN_DIR` selects another directory).
The AppImage starts `zarg`; invoked through a symlink named `zarc` (or with
`ZAR_CLI=1`) it starts the CLI.

The deb and rpm packages register Nautilus scripts and a Dolphin service menu,
including an action to extract into a folder named after the archive.
They also install a Thunar action template and the
`zarg-install-thunar-actions` helper. Thunar keeps actions in each user's
configuration, so run that helper once as the user to merge the ZAR actions
without replacing existing ones. Nautilus actions apply to local selections;
its script interface cannot represent filenames containing newlines.
The packages declare `application/x-zar` and install the supplied MIME icon.
AppImage and Flatpak include the MIME metadata and icon but do not register
host file-manager actions.

The Flatpak is built from source inside the Freedesktop SDK and downloads
crates during the build, so it is suitable for local bundles but not Flathub.
Its desktop file is named after the app ID, while `zarg` sets the window app ID
`zarg`, so some Wayland shells may not show the icon for the Flatpak window.

## macOS

```sh
packaging/macos/build-packages.sh [dmg|pkg|all]
```

Must run on macOS. Builds `Zarg.app` (with `zarc` bundled in
`Contents/Resources`) and produces `zar-rs-<version>-macos-<arch>.dmg` and
`.pkg`; the pkg also installs `zarc` to `/usr/local/bin` and Finder Quick
Actions under `/Library/Services`. The DMG includes the same workflows in a
`Finder Quick Actions` folder; copy them to `~/Library/Services` to install
them for only the current user. The app registers `.zar` as an alternate
archive viewer without changing the default handler, using the icon derived
from `application-x-zar.png`. Packages are ad-hoc signed unless
`CODESIGN_IDENTITY` (Developer ID Application) and
`INSTALLER_IDENTITY` (Developer ID Installer) are set. Notarization is not
included.
