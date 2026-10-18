; Installer Windows TilDown (Inno Setup 6).
;   iscc /DVersion=0.1.0 installer\tildown.iss   ->  target\installer\TilDown-0.1.0-setup.exe
; Butuh target\release\tildown.exe (cargo build --release).
; Per pengguna, tanpa admin. Catatan di Documents\TilDown tidak disentuh saat uninstall.

#ifndef Version
  #define Version "0.1.0"
#endif

[Setup]
AppId={{6B0F3C1E-8E1D-4C7A-9F35-2D7A1C9B4E61}
AppName=TilDown
AppVersion={#Version}
AppPublisher=arief nur andono
AppPublisherURL=https://github.com/goldalworming/tildown
DefaultDirName={autopf}\TilDown
DefaultGroupName=TilDown
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir=..\target\installer
OutputBaseFilename=TilDown-{#Version}-setup
SetupIconFile=tildown.ico
UninstallDisplayIcon={app}\tildown.exe
LicenseFile=..\LICENSE
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\target\release\tildown.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "tildown.ico"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\TilDown"; Filename: "{app}\tildown.exe"; IconFilename: "{app}\tildown.ico"
Name: "{autodesktop}\TilDown"; Filename: "{app}\tildown.exe"; IconFilename: "{app}\tildown.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\tildown.exe"; Description: "{cm:LaunchProgram,TilDown}"; Flags: nowait postinstall skipifsilent
