; Per-user installer. Store packaging remains a separate distribution path.
#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef BuildNumber
  #error BuildNumber is required
#endif
#ifndef Source
  #error Source is required
#endif

[Setup]
AppId={{B9CF57F1-A083-4B7D-BE98-B1F0C466EBB4}
AppName=MyImage
AppVersion={#AppVersion}
AppVerName=MyImage {#AppVersion} (build {#BuildNumber})
AppPublisher=Orhan Gökçöl
AppPublisherURL=https://github.com/gokcol/MyImage
AppSupportURL=https://github.com/gokcol/MyImage/issues
AppUpdatesURL=https://github.com/gokcol/MyImage/releases
VersionInfoVersion={#AppVersion}.{#BuildNumber}
VersionInfoProductName=MyImage
VersionInfoDescription=MyImage installer
DefaultDirName={localappdata}\Programs\MyImage
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.19041
SetupIconFile={#Source}\MyImage.ico
UninstallDisplayIcon={app}\MyImage.ico
UninstallDisplayName=MyImage
LicenseFile={#Source}\LICENSE.txt
InfoBeforeFile={#Source}\READ-ME-WINDOWS.txt
ChangesAssociations=yes
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "turkish"; MessagesFile: "compiler:Languages\Turkish.isl"

[CustomMessages]
english.OpenWith=Add MyImage to the Open with menu for projects and images
turkish.OpenWith=MyImage'i proje ve görsellerin Birlikte aç menüsüne ekle

[Tasks]
Name: "openwith"; Description: "{cm:OpenWith}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#Source}\MyImage.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\MyImage.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\Twemoji-LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\Twemoji-ATTRIBUTION.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\READ-ME-WINDOWS.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\MyImage"; Filename: "{app}\MyImage.exe"; IconFilename: "{app}\MyImage.ico"
Name: "{userdesktop}\MyImage"; Filename: "{app}\MyImage.exe"; IconFilename: "{app}\MyImage.ico"; Tasks: desktopicon

[Registry]
; Open with only: never take over a user's default image viewer.
Root: HKCU; Subkey: "Software\Classes\MyImage.Installer.Document"; ValueType: string; ValueName: ""; ValueData: "MyImage image or project"; Flags: uninsdeletekey; Tasks: openwith
Root: HKCU; Subkey: "Software\Classes\MyImage.Installer.Document\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\MyImage.ico"; Tasks: openwith
Root: HKCU; Subkey: "Software\Classes\MyImage.Installer.Document\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\MyImage.exe"" ""%1"""; Tasks: openwith
Root: HKCU; Subkey: "Software\Classes\.myimage\OpenWithProgids"; ValueType: string; ValueName: "MyImage.Installer.Document"; ValueData: ""; Flags: uninsdeletevalue; Tasks: openwith
Root: HKCU; Subkey: "Software\Classes\.png\OpenWithProgids"; ValueType: string; ValueName: "MyImage.Installer.Document"; ValueData: ""; Flags: uninsdeletevalue; Tasks: openwith
Root: HKCU; Subkey: "Software\Classes\.jpg\OpenWithProgids"; ValueType: string; ValueName: "MyImage.Installer.Document"; ValueData: ""; Flags: uninsdeletevalue; Tasks: openwith
Root: HKCU; Subkey: "Software\Classes\.jpeg\OpenWithProgids"; ValueType: string; ValueName: "MyImage.Installer.Document"; ValueData: ""; Flags: uninsdeletevalue; Tasks: openwith
Root: HKCU; Subkey: "Software\Classes\.webp\OpenWithProgids"; ValueType: string; ValueName: "MyImage.Installer.Document"; ValueData: ""; Flags: uninsdeletevalue; Tasks: openwith
Root: HKCU; Subkey: "Software\Classes\.bmp\OpenWithProgids"; ValueType: string; ValueName: "MyImage.Installer.Document"; ValueData: ""; Flags: uninsdeletevalue; Tasks: openwith
Root: HKCU; Subkey: "Software\Classes\.gif\OpenWithProgids"; ValueType: string; ValueName: "MyImage.Installer.Document"; ValueData: ""; Flags: uninsdeletevalue; Tasks: openwith
Root: HKCU; Subkey: "Software\Classes\.tif\OpenWithProgids"; ValueType: string; ValueName: "MyImage.Installer.Document"; ValueData: ""; Flags: uninsdeletevalue; Tasks: openwith
Root: HKCU; Subkey: "Software\Classes\.tiff\OpenWithProgids"; ValueType: string; ValueName: "MyImage.Installer.Document"; ValueData: ""; Flags: uninsdeletevalue; Tasks: openwith

[Run]
Filename: "{app}\MyImage.exe"; Description: "{cm:LaunchProgram,MyImage}"; Flags: nowait postinstall skipifsilent

; No UninstallDelete entries: projects, captures, settings, recovery and unknown
; files are deliberately preserved. Login startup stays off by default.

[Code]
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Command: String;
begin
  if CurUninstallStep = usUninstall then
    if RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'MyImage', Command) then
      if CompareText(Command, '"' + ExpandConstant('{app}\MyImage.exe') + '"') = 0 then
        RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'MyImage');
end;
