; Inno Setup 6 script for Throne (Zed-style Windows packaging).
; Built by script/bundle-windows.ps1 — do not run ISCC by hand unless vars are set.
;
; Required defines (passed via /D):
;   AppVersion, Arch, SourceDir, OutputDir, OutputBaseFilename

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef Arch
  #define Arch "x86_64"
#endif
#ifndef SourceDir
  #define SourceDir "..\..\..\target\release"
#endif
#ifndef OutputDir
  #define OutputDir "..\..\..\dist"
#endif
#ifndef OutputBaseFilename
  #define OutputBaseFilename "Throne-" + Arch
#endif

#define MyAppName "Throne"
#define MyAppPublisher "Throne-rs"
#define MyAppURL "https://github.com/kandyjam/throne-rs"
#define MyAppExeName "Throne.exe"

[Setup]
AppId={{A7C3E9F1-4B2D-4E8A-9C1F-THRONE0RS001}
AppName={#MyAppName}
AppVersion={#AppVersion}
AppVerName={#MyAppName} {#AppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
LicenseFile=..\..\..\LICENSE
OutputDir={#OutputDir}
OutputBaseFilename={#OutputBaseFilename}
SetupIconFile=..\app-icon.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible arm64
#if Arch == "aarch64"
  ArchitecturesInstallIn64BitMode=arm64
#else
  ArchitecturesInstallIn64BitMode=x64compatible
#endif
CloseApplications=yes
RestartApplications=no
ChangesAssociations=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
; GUI + core must share {app} (upstream parentcheck: basename Throne + ThroneCore beside it)
Source: "{#SourceDir}\Throne.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\ThroneCore.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\app-icon.ico"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Registry]
; throne:// deeplink
Root: HKCU; Subkey: "Software\Classes\throne"; ValueType: string; ValueName: ""; ValueData: "URL:Throne Protocol"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\throne"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\throne\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" ""%1"""

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent
