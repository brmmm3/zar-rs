; Inno Setup script for the ZAR command-line tools (zarc, zcopy), archive manager
; (zarg) and FUSE filesystem (zarmount).
;
; The installer supports both per-user (default, no elevation needed) and
; all-users (requires admin) installation.  Select "Install for all users" in
; the tasks page or pass /ALLUSERS on the command line.
;
; Build (from the repository root, after `cargo build --release --bins`):
;   iscc /DAppVersion=0.17.1 packaging\windows\zar-rs.iss
; Options:
;   /DSourceDir=<dir>   directory containing zarc.exe, zarg.exe, zcopy.exe and
;                       zarmount.exe (default: target\release; use
;                       target\<triple>\release when building with --target)
;   /DArch=arm64        build the installer for Windows on ARM (default: x64)

#ifndef AppVersion
  #define AppVersion "0.17.1"
#endif
#ifndef SourceDir
  #define SourceDir "..\..\target\release"
#endif
#ifndef Arch
  #define Arch "x64"
#endif

#define AppName "ZAR"
#define AppPublisher "Martin Bammer"
#define AppURL "https://github.com/brmmm3/zar-rs"
#define SupportedArchivesAppliesTo "System.FileExtension:=.zar OR System.FileExtension:=.zip OR System.FileExtension:=.7z OR System.FileExtension:=.rar OR System.FileExtension:=.iso OR System.FileExtension:=.tar OR System.FileExtension:=.tgz OR System.FileExtension:=.gz"
#define ConvertToZarAppliesTo "System.ItemType:=Directory OR System.FileExtension:=.zip OR System.FileExtension:=.7z OR System.FileExtension:=.rar OR System.FileExtension:=.iso OR System.FileExtension:=.tar OR System.FileExtension:=.tgz OR System.FileExtension:=.gz"

[Setup]
AppId={{6F0B7A5C-3E57-4D5E-9B0A-5A1D6E2F7C11}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppURL}
AppSupportURL={#AppURL}/issues
AppUpdatesURL={#AppURL}/releases
DefaultDirName={autopf}\ZAR
DefaultGroupName=ZAR
DisableProgramGroupPage=yes
LicenseFile=..\..\LICENSE-MIT
SetupIconFile=..\..\zarlib-gui\assets\zarg-icon.ico
OutputDir=..\..\dist
OutputBaseFilename=zar-rs-{#AppVersion}-windows-{#Arch}-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayName={#AppName} {#AppVersion}
UninstallDisplayIcon={app}\zarg-icon.ico
ChangesAssociations=yes
ChangesEnvironment=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=commandline dialog
#if Arch == "arm64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "addtopath"; Description: "Add the &zarc and zcopy command-line tools to the PATH"; GroupDescription: "Command line:"
Name: "allusers"; Description: "Install for &all users"; GroupDescription: "Installation scope:"; Flags: unchecked
Name: "assoczar"; Description: "Associate &.zar files with the archive manager"; GroupDescription: "File associations:"; Flags: unchecked
Name: "desktopicon"; Description: "Create a &desktop shortcut for the archive manager"; GroupDescription: "Additional shortcuts:"; Flags: unchecked

[Files]
Source: "{#SourceDir}\zarc.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\zarg.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\zcopy.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\zarmount.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\zarlib-gui\assets\zarg-icon.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\zarlib-cli\README.md"; DestDir: "{app}"; DestName: "zarc-README.md"; Flags: ignoreversion
Source: "..\..\zarlib-gui\README.md"; DestDir: "{app}"; DestName: "zarg-README.md"; Flags: ignoreversion
Source: "..\..\zcopy\README.md"; DestDir: "{app}"; DestName: "zcopy-README.md"; Flags: ignoreversion

[Icons]
Name: "{group}\ZAR Archive Manager"; Filename: "{app}\zarg.exe"; IconFilename: "{app}\zarg-icon.ico"
Name: "{autodesktop}\ZAR Archive Manager"; Filename: "{app}\zarg.exe"; IconFilename: "{app}\zarg-icon.ico"; Tasks: desktopicon

[Registry]
; System PATH (all-users install)
Root: HKLM; Subkey: "SYSTEM\CurrentControlSet\Control\Session Manager\Environment"; \
  ValueType: expandsz; ValueName: "Path"; ValueData: "{olddata};{app}"; \
  Tasks: addtopath and allusers; Check: NeedsAddPath(ExpandConstant('{app}'))
; User PATH (per-user install)
Root: HKCU; Subkey: "Environment"; \
  ValueType: expandsz; ValueName: "Path"; ValueData: "{olddata};{app}"; \
  Tasks: addtopath; Check: not IsAllUsers and NeedsAddPathUser(ExpandConstant('{app}'))

; ---------------------------------------------------------------------------
; ZAR context-menu submenu for all filesystem objects.
;
; The top-level "ZAR" entry uses Explorer's SubCommands registry pattern to
; produce a cascading menu.  Each verb uses the "Player" selection model so
; that multiple selected paths arrive together (including paths with spaces).
;
; When the "allusers" task is selected the entries go under HKLM (machine-wide);
; otherwise they go under HKCU (current user only, no elevation needed).
; ---------------------------------------------------------------------------
#define ClassesRoot "Software\Classes"

Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR"; \
  ValueType: string; ValueName: "MUIVerb"; ValueData: "ZAR"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR"; \
  ValueType: string; ValueName: "Icon"; ValueData: "{app}\zarg-icon.ico"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR"; \
  ValueType: string; ValueName: "SubCommands"; ValueData: ""; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR"; \
  ValueType: string; ValueName: "ExtendedSubCommandsKey"; ValueData: ""; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR"; \
  ValueType: string; ValueName: "MUIVerb"; ValueData: "ZAR"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR"; \
  ValueType: string; ValueName: "Icon"; ValueData: "{app}\zarg-icon.ico"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR"; \
  ValueType: string; ValueName: "SubCommands"; ValueData: ""; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR"; \
  ValueType: string; ValueName: "ExtendedSubCommandsKey"; ValueData: ""; \
  Flags: uninsdeletekey; Check: not IsAllUsers

; --- Clean up legacy/stale context menu entries ---
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\-sep1"; \
  ValueType: none; Flags: deletekey uninsdeletekey; Check: IsAdminInstallMode
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\-sep2"; \
  ValueType: none; Flags: deletekey uninsdeletekey; Check: IsAdminInstallMode
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\04sep1"; \
  ValueType: none; Flags: deletekey uninsdeletekey; Check: IsAdminInstallMode
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\06add"; \
  ValueType: none; Flags: deletekey uninsdeletekey; Check: IsAdminInstallMode
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\07createnamed"; \
  ValueType: none; Flags: deletekey uninsdeletekey; Check: IsAdminInstallMode
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\08sep2"; \
  ValueType: none; Flags: deletekey uninsdeletekey; Check: IsAdminInstallMode
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\-sep1"; \
  ValueType: none; Flags: deletekey uninsdeletekey
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\-sep2"; \
  ValueType: none; Flags: deletekey uninsdeletekey
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\04sep1"; \
  ValueType: none; Flags: deletekey uninsdeletekey
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\06add"; \
  ValueType: none; Flags: deletekey uninsdeletekey
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\07createnamed"; \
  ValueType: none; Flags: deletekey uninsdeletekey
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\08sep2"; \
  ValueType: none; Flags: deletekey uninsdeletekey

; --- Open Archive ---
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\01open"; \
  ValueType: string; ValueName: "MUIVerb"; ValueData: "Open Archive"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\01open"; \
  ValueType: string; ValueName: "AppliesTo"; ValueData: "{#SupportedArchivesAppliesTo}"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\01open"; \
  ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\01open\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\zarg.exe"" --context open -- ""%1"""; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\01open"; \
  ValueType: string; ValueName: "MUIVerb"; ValueData: "Open Archive"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\01open"; \
  ValueType: string; ValueName: "AppliesTo"; ValueData: "{#SupportedArchivesAppliesTo}"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\01open"; \
  ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\01open\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\zarg.exe"" --context open -- ""%1"""; \
  Flags: uninsdeletekey; Check: not IsAllUsers

; --- Extract here ---
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\02extracthere"; \
  ValueType: string; ValueName: "MUIVerb"; ValueData: "Extract here"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\02extracthere"; \
  ValueType: string; ValueName: "AppliesTo"; ValueData: "{#SupportedArchivesAppliesTo}"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\02extracthere"; \
  ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\02extracthere\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\zarg.exe"" --context extract-here -- ""%1"""; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\02extracthere"; \
  ValueType: string; ValueName: "MUIVerb"; ValueData: "Extract here"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\02extracthere"; \
  ValueType: string; ValueName: "AppliesTo"; ValueData: "{#SupportedArchivesAppliesTo}"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\02extracthere"; \
  ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\02extracthere\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\zarg.exe"" --context extract-here -- ""%1"""; \
  Flags: uninsdeletekey; Check: not IsAllUsers

; --- Extract to... ---
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\03extractto"; \
  ValueType: string; ValueName: "MUIVerb"; ValueData: "Extract to..."; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\03extractto"; \
  ValueType: string; ValueName: "AppliesTo"; ValueData: "{#SupportedArchivesAppliesTo}"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\03extractto"; \
  ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\03extractto\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\zarg.exe"" --context extract-to -- ""%1"""; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\03extractto"; \
  ValueType: string; ValueName: "MUIVerb"; ValueData: "Extract to..."; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\03extractto"; \
  ValueType: string; ValueName: "AppliesTo"; ValueData: "{#SupportedArchivesAppliesTo}"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\03extractto"; \
  ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\03extractto\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\zarg.exe"" --context extract-to -- ""%1"""; \
  Flags: uninsdeletekey; Check: not IsAllUsers

; --- Create Archive... ---
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\05create"; \
  ValueType: dword; ValueName: "CommandFlags"; ValueData: "$20"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\05create"; \
  ValueType: dword; ValueName: "CommandFlags"; ValueData: "$20"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\05create"; \
  ValueType: string; ValueName: "MUIVerb"; ValueData: "Create Archive..."; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\05create"; \
  ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\05create\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\zarg.exe"" --context create -- ""%1"""; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\05create"; \
  ValueType: string; ValueName: "MUIVerb"; ValueData: "Create Archive..."; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\05create"; \
  ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\05create\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\zarg.exe"" --context create -- ""%1"""; \
  Flags: uninsdeletekey; Check: not IsAllUsers

; --- Convert to ZAR ---
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\06convert"; \
  ValueType: string; ValueName: "MUIVerb"; ValueData: "Convert to ZAR"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\06convert"; \
  ValueType: string; ValueName: "AppliesTo"; ValueData: "{#ConvertToZarAppliesTo}"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\06convert"; \
  ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\06convert\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\zarg.exe"" --context convert-to-zar -- ""%1"""; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\06convert"; \
  ValueType: string; ValueName: "MUIVerb"; ValueData: "Convert to ZAR"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\06convert"; \
  ValueType: string; ValueName: "AppliesTo"; ValueData: "{#ConvertToZarAppliesTo}"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\06convert"; \
  ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\06convert\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\zarg.exe"" --context convert-to-zar -- ""%1"""; \
  Flags: uninsdeletekey; Check: not IsAllUsers

; --- Mount Archive ---
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\09mount"; \
  ValueType: dword; ValueName: "CommandFlags"; ValueData: "$20"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\09mount"; \
  ValueType: dword; ValueName: "CommandFlags"; ValueData: "$20"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\09mount"; \
  ValueType: string; ValueName: "MUIVerb"; ValueData: "Mount Archive"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\09mount"; \
  ValueType: string; ValueName: "AppliesTo"; ValueData: "System.FileExtension:=.zar"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\09mount"; \
  ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\09mount\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\zarmount.exe"" --ask ""%1"""; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\09mount"; \
  ValueType: string; ValueName: "MUIVerb"; ValueData: "Mount Archive"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\09mount"; \
  ValueType: string; ValueName: "AppliesTo"; ValueData: "System.FileExtension:=.zar"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\09mount"; \
  ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\AllFilesystemObjects\shell\ZAR\shell\09mount\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\zarmount.exe"" --ask ""%1"""; \
  Flags: uninsdeletekey; Check: not IsAllUsers

; ---------------------------------------------------------------------------
; .zar file-specific verbs (appear directly on .zar files in addition to the
; ZAR submenu above).
; ---------------------------------------------------------------------------
Root: HKLM; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.Open"; \
  ValueType: string; ValueName: ""; ValueData: "Open Archive"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.Open"; \
  ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.Open\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\zarg.exe"" --context open -- ""%1"""; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.Open"; \
  ValueType: string; ValueName: ""; ValueData: "Open Archive"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.Open"; \
  ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.Open\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\zarg.exe"" --context open -- ""%1"""; \
  Flags: uninsdeletekey; Check: not IsAllUsers

Root: HKLM; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.ExtractHere"; \
  ValueType: string; ValueName: ""; ValueData: "Extract here"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.ExtractHere"; \
  ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.ExtractHere\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\zarg.exe"" --context extract-here -- ""%1"""; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.ExtractHere"; \
  ValueType: string; ValueName: ""; ValueData: "Extract here"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.ExtractHere"; \
  ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.ExtractHere\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\zarg.exe"" --context extract-here -- ""%1"""; \
  Flags: uninsdeletekey; Check: not IsAllUsers

Root: HKLM; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.ExtractTo"; \
  ValueType: string; ValueName: ""; ValueData: "Extract to..."; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.ExtractTo"; \
  ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.ExtractTo\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\zarg.exe"" --context extract-to -- ""%1"""; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.ExtractTo"; \
  ValueType: string; ValueName: ""; ValueData: "Extract to..."; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.ExtractTo"; \
  ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.ExtractTo\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\zarg.exe"" --context extract-to -- ""%1"""; \
  Flags: uninsdeletekey; Check: not IsAllUsers

Root: HKLM; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.ExtractToFolder"; \
  ValueType: string; ValueName: ""; ValueData: "Extract to archive folder"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.ExtractToFolder"; \
  ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.ExtractToFolder\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\zarg.exe"" --context extract-to-folder -- ""%1"""; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.ExtractToFolder"; \
  ValueType: string; ValueName: ""; ValueData: "Extract to archive folder"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.ExtractToFolder"; \
  ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.ExtractToFolder\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\zarg.exe"" --context extract-to-folder -- ""%1"""; \
  Flags: uninsdeletekey; Check: not IsAllUsers

Root: HKLM; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.Test"; \
  ValueType: string; ValueName: ""; ValueData: "Test Archive"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.Test"; \
  ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKLM; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.Test\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\zarg.exe"" --context test -- ""%1"""; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.Test"; \
  ValueType: string; ValueName: ""; ValueData: "Test Archive"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.Test"; \
  ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"; \
  Flags: uninsdeletekey; Check: not IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\SystemFileAssociations\.zar\shell\ZAR.Test\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\zarg.exe"" --context test -- ""%1"""; \
  Flags: uninsdeletekey; Check: not IsAllUsers

Root: HKLM; Subkey: "{#ClassesRoot}\.zar\DefaultIcon"; \
  ValueType: string; ValueName: ""; ValueData: "{app}\zarg-icon.ico"; \
  Flags: uninsdeletekey; Tasks: allusers; Check: IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\.zar\DefaultIcon"; \
  ValueType: string; ValueName: ""; ValueData: "{app}\zarg-icon.ico"; \
  Flags: uninsdeletekey; Check: not IsAllUsers

; ---------------------------------------------------------------------------
; .zar ProgID — registers zarg as the default handler so double-clicking a
; .zar file opens it in the archive manager.
; ---------------------------------------------------------------------------
Root: HKLM; Subkey: "{#ClassesRoot}\.zar\OpenWithProgids"; \
  ValueType: string; ValueName: "ZAR.Archive"; ValueData: ""; \
  Flags: uninsdeletevalue; Tasks: assoczar and allusers; Check: IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\.zar\OpenWithProgids"; \
  ValueType: string; ValueName: "ZAR.Archive"; ValueData: ""; \
  Flags: uninsdeletevalue; Tasks: assoczar; Check: not IsAllUsers

Root: HKLM; Subkey: "{#ClassesRoot}\ZAR.Archive"; \
  ValueType: string; ValueName: ""; ValueData: "ZAR Archive"; \
  Flags: uninsdeletekey; Tasks: assoczar and allusers; Check: IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\ZAR.Archive"; \
  ValueType: string; ValueName: ""; ValueData: "ZAR Archive"; \
  Flags: uninsdeletekey; Tasks: assoczar; Check: not IsAllUsers

Root: HKLM; Subkey: "{#ClassesRoot}\ZAR.Archive\DefaultIcon"; \
  ValueType: string; ValueName: ""; ValueData: "{app}\zarg-icon.ico"; \
  Flags: uninsdeletekey; Tasks: assoczar and allusers; Check: IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\ZAR.Archive\DefaultIcon"; \
  ValueType: string; ValueName: ""; ValueData: "{app}\zarg-icon.ico"; \
  Flags: uninsdeletekey; Tasks: assoczar; Check: not IsAllUsers

Root: HKLM; Subkey: "{#ClassesRoot}\ZAR.Archive\shell\open\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\zarg.exe"" --context open -- ""%1"""; \
  Flags: uninsdeletekey; Tasks: assoczar and allusers; Check: IsAllUsers
Root: HKCU; Subkey: "{#ClassesRoot}\ZAR.Archive\shell\open\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\zarg.exe"" --context open -- ""%1"""; \
  Flags: uninsdeletekey; Tasks: assoczar; Check: not IsAllUsers

[Run]
Filename: "{app}\zarg.exe"; Description: "Launch ZAR Archive Manager"; Flags: nowait postinstall skipifsilent

[Code]
const
  SystemEnvironmentKey = 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment';
  UserEnvironmentKey = 'Environment';

function IsAllUsers: Boolean;
begin
  Result := IsTaskSelected('allusers');
end;

function GetDefaultDirName(Param: string): string;
begin
  if IsAllUsers then
    Result := ExpandConstant('{autopf}\ZAR')
  else
    Result := ExpandConstant('{localappdata}\ZAR');
end;

function NeedsAddPath(Param: string): Boolean;
var
  OrigPath: string;
begin
  if not RegQueryStringValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'Path', OrigPath) then
  begin
    Result := True;
    Exit;
  end;
  Result := Pos(';' + Uppercase(Param) + ';', ';' + Uppercase(OrigPath) + ';') = 0;
end;

function NeedsAddPathUser(Param: string): Boolean;
var
  OrigPath: string;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, UserEnvironmentKey, 'Path', OrigPath) then
  begin
    Result := True;
    Exit;
  end;
  Result := Pos(';' + Uppercase(Param) + ';', ';' + Uppercase(OrigPath) + ';') = 0;
end;

procedure RemoveFromPath(const Dir: string);
var
  OrigPath, Needle, Padded: string;
  Position: Integer;
begin
  if IsAllUsers then
  begin
    if not RegQueryStringValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'Path', OrigPath) then
      Exit;
  end
  else
  begin
    if not RegQueryStringValue(HKEY_CURRENT_USER, UserEnvironmentKey, 'Path', OrigPath) then
      Exit;
  end;
  Needle := ';' + Uppercase(Dir) + ';';
  Padded := ';' + OrigPath + ';';
  Position := Pos(Needle, Uppercase(Padded));
  if Position = 0 then
    Exit;
  Delete(Padded, Position, Length(Needle) - 1);
  // Padded now starts and ends with a separator that was only added for matching.
  if (Length(Padded) > 0) and (Padded[1] = ';') then
    Delete(Padded, 1, 1);
  if (Length(Padded) > 0) and (Padded[Length(Padded)] = ';') then
    Delete(Padded, Length(Padded), 1);
  if IsAllUsers then
    RegWriteExpandStringValue(HKEY_LOCAL_MACHINE, SystemEnvironmentKey, 'Path', Padded)
  else
    RegWriteExpandStringValue(HKEY_CURRENT_USER, UserEnvironmentKey, 'Path', Padded);
end;

procedure CleanUpLegacyKeys;
begin
  RegDeleteKeyIncludingSubkeys(HKEY_LOCAL_MACHINE, 'Software\Classes\AllFilesystemObjects\shell\ZAR\shell\04sep1');
  RegDeleteKeyIncludingSubkeys(HKEY_LOCAL_MACHINE, 'Software\Classes\AllFilesystemObjects\shell\ZAR\shell\06add');
  RegDeleteKeyIncludingSubkeys(HKEY_LOCAL_MACHINE, 'Software\Classes\AllFilesystemObjects\shell\ZAR\shell\07createnamed');
  RegDeleteKeyIncludingSubkeys(HKEY_LOCAL_MACHINE, 'Software\Classes\AllFilesystemObjects\shell\ZAR\shell\08sep2');
  RegDeleteKeyIncludingSubkeys(HKEY_LOCAL_MACHINE, 'Software\Classes\AllFilesystemObjects\shell\ZAR\shell\-sep1');
  RegDeleteKeyIncludingSubkeys(HKEY_LOCAL_MACHINE, 'Software\Classes\AllFilesystemObjects\shell\ZAR\shell\-sep2');
  RegDeleteKeyIncludingSubkeys(HKEY_CURRENT_USER, 'Software\Classes\AllFilesystemObjects\shell\ZAR\shell\04sep1');
  RegDeleteKeyIncludingSubkeys(HKEY_CURRENT_USER, 'Software\Classes\AllFilesystemObjects\shell\ZAR\shell\06add');
  RegDeleteKeyIncludingSubkeys(HKEY_CURRENT_USER, 'Software\Classes\AllFilesystemObjects\shell\ZAR\shell\07createnamed');
  RegDeleteKeyIncludingSubkeys(HKEY_CURRENT_USER, 'Software\Classes\AllFilesystemObjects\shell\ZAR\shell\08sep2');
  RegDeleteKeyIncludingSubkeys(HKEY_CURRENT_USER, 'Software\Classes\AllFilesystemObjects\shell\ZAR\shell\-sep1');
  RegDeleteKeyIncludingSubkeys(HKEY_CURRENT_USER, 'Software\Classes\AllFilesystemObjects\shell\ZAR\shell\-sep2');
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssInstall then
    CleanUpLegacyKeys;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
  begin
    RemoveFromPath(ExpandConstant('{app}'));
    CleanUpLegacyKeys;
  end;
end;
