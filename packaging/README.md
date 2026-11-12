# Packaging

Installers for `zarc` (CLI) and `zarg` (GUI). Output goes to `dist/`.

The release workflows reuse binaries from the latest successful CI run on
`master` for the exact tagged commit when those artifacts are still retained.
If no matching artifact is available, the workflow builds the binaries from
source. Flatpak continues to build inside its Freedesktop SDK.

The separate `x.y.z-v1` release includes baseline x86-64 packages built by
`.github/workflows/packaging-v1.yml`. It reuses only the matching v1 release
binaries, builds the Windows `zcopy` utility for the same CPU baseline, and
sets baseline compiler flags inside the Flatpak SDK. Package filenames carry a
`-v1` suffix; their internal package versions are unchanged. OS, system
library, and Flatpak runtime requirements still apply. The workflow runs after
the v1 release workflow or can be dispatched manually with its source tag.

## Windows (Inno Setup)

```powershell
cargo build --release --bins
iscc /DAppVersion=0.9.0 packaging\windows\zar-rs.iss
```

Use `/DSourceDir=target\<triple>\release` when building with `--target`, and
`/DArch=arm64` for Windows on ARM. The installer can add `zarc` to the system
`PATH`, create Start menu and desktop shortcuts, and register Explorer verbs
for files, folders and `.zar` archives. Explorer passes multiple selections to
the create actions. Windows does not support substituting the selected
filename into a registry verb's display text, so the named actions use fixed
labels. **Create Archive (named)** creates `<first file's stem>.zar` or
`<first folder's name>.zar` beside the first selected item. **Extract to archive
folder** extracts `foo.zar` into a sibling folder named `foo`. The `.zar` file
icon is generated from the PNG MIME icon.

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

Each package can be built and installed locally, for example:

```powershell
choco pack packaging\windows\chocolatey\zarc\zarc.nuspec --output-directory "$env:TEMP"
choco install zarc --version 0.11.0 --source "$env:TEMP" -y
```

The `zarc` and `zarg` packages install portable executables with Chocolatey
shims. The `zcopy` package requires administrator privileges and installs the
complete ZAR suite with Explorer integration and `zarc` and `zcopy` on the
system PATH. Uninstalling `zcopy` removes that suite. Avoid installing the
suite through multiple package managers, or combining it with the portable
packages, to prevent duplicate executables and conflicting ownership.

### Scoop

Install a local manifest, for example:

```powershell
scoop install .\packaging\windows\scoop\zarg.json
```

The `zarc` and `zarg` manifests use the release ZIPs. Since those ZIPs do
not contain `zcopy`, its manifest extracts the suite installer with Scoop's
Inno Setup helper and exposes only `zcopy` as a command. Scoop's portable
packages do not register Explorer integration. Run `scoop update` if stale
bucket metadata causes the helper's checksum verification to fail. The Scoop
and Chocolatey packages have not been published to their community
repositories. Update versions, URLs and checksums for each release.

## Linux

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
