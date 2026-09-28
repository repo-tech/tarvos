#define MyAppName "Tarvos"
#define MyAppVersion "1.1.0-rc.4"
#define MyAppPublisher "Repo-Tech"
#define MyAppExeName "tarvos.exe"

[Setup]
AppId={{8E6B3D86-9F54-4B74-9A5E-2B2B7D7A15A1}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={userappdata}\Tarvos\bin
DefaultGroupName=Tarvos
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=output
OutputBaseFilename=Tarvos-Setup-Windows-x86_64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
Uninstallable=yes
ArchitecturesInstallIn64BitMode=x64

[Files]
Source: "payload\tarvos.exe"; DestDir: "{app}"; Flags: ignoreversion

[Registry]
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; ValueData: "{app};{olddata}"; Flags: preservestringtype

[Icons]
Name: "{group}\Tarvos"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"

[Run]
Filename: "{app}\{#MyAppExeName}"; Parameters: "--version"; Description: "Verify Tarvos installation"; Flags: postinstall skipifsilent
; Record the optional local AI capability. This step is read-only: it starts,
; stops, downloads and configures nothing, only probes a loopback endpoint, and
; always exits 0 because the capability is optional. `runhidden` keeps it from
; flashing a console during a normal install, and it runs on every install
; (including silent) rather than only post-install.
Filename: "{app}\{#MyAppExeName}"; Parameters: "ai-status"; Description: "Record optional local AI capability"; Flags: runhidden skipifdoesntexist
