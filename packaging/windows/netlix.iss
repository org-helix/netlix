; netlix installer for Windows (Inno Setup 6).
;
; Build netlix first, then from the repository root:
;   iscc packaging\windows\netlix.iss
; The installer lands in dist\netlix-windows-setup.exe.
;
; Installs for the current user by default: no administrator rights, no UAC prompt.
; IT staff can install for every user of the machine with:  netlix-windows-setup.exe /ALLUSERS
; Silent install:  netlix-windows-setup.exe /VERYSILENT /SUPPRESSMSGBOXES

#define AppName "netlix"
#define Exe "..\..\target\release\netlix.exe"
#ifndef AppVersion
  #define AppVersion GetStringFileInfo(Exe, "ProductVersion")
#endif

[Setup]
; Never change AppId: it is how Windows recognises netlix for updates and uninstall.
AppId={{56FF4DCA-A728-415B-B66B-37F83141F509}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=Helix
AppPublisherURL=https://github.com/org-helix
AppSupportURL=https://github.com/org-helix/netlix/issues
AppUpdatesURL=https://github.com/org-helix/netlix/releases
VersionInfoVersion={#AppVersion}
VersionInfoProductName={#AppName}
VersionInfoDescription={#AppName} setup

DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
DisableDirPage=auto
DisableReadyPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=commandline
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0

ChangesAssociations=yes
CloseApplications=yes
UninstallDisplayIcon={app}\netlix.exe
UninstallDisplayName={#AppName}

WizardStyle=modern
SetupIconFile=..\..\assets\icon\netlix.ico
WizardImageFile=wizard-100.bmp,wizard-125.bmp,wizard-150.bmp,wizard-200.bmp
WizardSmallImageFile=wizard-small-100.bmp,wizard-small-125.bmp,wizard-small-150.bmp,wizard-small-200.bmp
ShowLanguageDialog=auto

OutputDir=..\..\dist
OutputBaseFilename=netlix-windows-setup
Compression=lzma2/max
SolidCompression=yes

[Languages]
Name: "fr"; MessagesFile: "compiler:Languages\French.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
fr.AssocDesc=Ouvrir les fichiers .netlix avec netlix
en.AssocDesc=Open .netlix files with netlix
fr.FileType=Topologie netlix
en.FileType=netlix topology

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "assoc"; Description: "{cm:AssocDesc}"

[Files]
Source: "{#Exe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\assets\fonts\OFL-*.txt"; DestDir: "{app}\licenses"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\netlix.exe"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\netlix.exe"; Tasks: desktopicon

[Registry]
; .netlix files open in netlix (HKA = current user, or the machine with /ALLUSERS).
Root: HKA; Subkey: "Software\Classes\.netlix"; ValueType: string; ValueName: ""; ValueData: "netlix.Topology"; Flags: uninsdeletevalue uninsdeletekeyifempty; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\.netlix\OpenWithProgids"; ValueType: string; ValueName: "netlix.Topology"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\netlix.Topology"; ValueType: string; ValueName: ""; ValueData: "{cm:FileType}"; Flags: uninsdeletekey; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\netlix.Topology\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\netlix.exe,0"; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\netlix.Topology\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\netlix.exe"" ""%1"""; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\Applications\netlix.exe"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\netlix.exe\SupportedTypes"; ValueType: string; ValueName: ".netlix"; ValueData: ""

[Run]
Filename: "{app}\netlix.exe"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent
