# Packaging

Installers for `zarc` (CLI) and `zarg` (GUI). Output goes to `dist/`.

## Windows (Inno Setup)

```powershell
cargo build --release --bins
iscc /DAppVersion=0.7.0 packaging\windows\zar-rs.iss
```

Use `/DSourceDir=target\<triple>\release` when building with `--target`, and
`/DArch=arm64` for Windows on ARM. The installer can add `zarc` to the system
`PATH`, create Start menu and desktop shortcuts, and register Explorer verbs
for files, folders and `.zar` archives. Explorer passes multiple selections to
the create actions. Windows does not support substituting the selected
filename into a registry verb's display text, so the named action uses the
fixed label **Create Archive (named)** on all platforms. It creates
`<first file's stem>.zar` or `<first folder's name>.zar` beside the first
selected item.

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

The deb and rpm packages register Nautilus scripts and a Dolphin service menu.
They also install a Thunar action template and the
`zarg-install-thunar-actions` helper. Thunar keeps actions in each user's
configuration, so run that helper once as the user to merge the ZAR actions
without replacing existing ones. Nautilus actions apply to local selections;
its script interface cannot represent filenames containing newlines.
AppImage and Flatpak do not register host file-manager actions.

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
archive viewer without changing the default handler. Packages are ad-hoc signed
unless `CODESIGN_IDENTITY` (Developer ID Application) and
`INSTALLER_IDENTITY` (Developer ID Installer) are set. Notarization is not
included.
