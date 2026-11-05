; Earshot's Windows installer (Inno Setup 6).
;   iscc /DMyAppVersion=0.1.0 /DSourceDir=C:\path\to\deployed\Earshot packaging\windows\earshot.iss

#ifndef MyAppVersion
  #define MyAppVersion "0.0.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\..\dist\Earshot"
#endif

[Setup]
AppId={{B8E3B6F2-4C1A-4E7B-9D61-3F2A9C5E7D10}
AppName=Earshot
AppVersion={#MyAppVersion}
AppVerName=Earshot {#MyAppVersion}
AppPublisher=Ben Booth
AppPublisherURL=https://benwbooth.github.io/earshot/
AppSupportURL=https://github.com/benwbooth/earshot/issues
DefaultDirName={autopf}\Earshot
DefaultGroupName=Earshot
DisableProgramGroupPage=yes
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputBaseFilename=Earshot-{#MyAppVersion}-windows-x64-setup
SetupIconFile=..\earshot.ico
UninstallDisplayIcon={app}\earshot.exe
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\Earshot"; Filename: "{app}\earshot.exe"
Name: "{autodesktop}\Earshot"; Filename: "{app}\earshot.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\earshot.exe"; Description: "{cm:LaunchProgram,Earshot}"; Flags: nowait postinstall skipifsilent
