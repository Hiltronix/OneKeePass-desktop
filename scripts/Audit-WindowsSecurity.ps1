# Hiltronix OneKeePass Windows runtime security audit.
# Read-only: does not alter firewall, WFP audit policy, registry, or SSH keys.
# Run in Windows PowerShell 5.1 or PowerShell 7 while OneKeePass is running.
# TCP is periodically sampled, so brief connections and UDP sends may be missed.
param(
    [ValidateRange(5,3600)][int]$Seconds = 120,
    [ValidateRange(1,60)][int]$SampleIntervalSeconds = 1,
    [string]$OutputDirectory = ""
)
$ErrorActionPreference = "Stop"
if ($env:OS -ne "Windows_NT") { throw "This audit requires Windows." }
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $env:TEMP ("OKP-Windows-Audit-" + (Get-Date -Format "yyyyMMdd-HHmmss"))
}
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null

# Query the security descriptor of the existing pipe by name. No agent protocol
# request or signing operation is sent. Capture the SDDL/ACEs, not private keys.
$native = @'
using System;
using System.Runtime.InteropServices;
public static class OkpPipeSecurity {
    [DllImport("kernel32.dll", EntryPoint="CreateFileW", CharSet=CharSet.Unicode, SetLastError=true)]
    public static extern IntPtr CreateFile(
        string name, uint desiredAccess, uint shareMode, IntPtr securityAttributes,
        uint creationDisposition, uint flagsAndAttributes, IntPtr templateFile);
    [DllImport("kernel32.dll", SetLastError=true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool CloseHandle(IntPtr handle);
    [DllImport("advapi32.dll", EntryPoint="GetSecurityInfo")]
    public static extern uint GetSecurityInfo(
        IntPtr handle, int objectType, uint securityInfo, out IntPtr owner,
        out IntPtr group, out IntPtr dacl, out IntPtr sacl, out IntPtr descriptor);
    [DllImport("advapi32.dll", EntryPoint="ConvertSecurityDescriptorToStringSecurityDescriptorW",
        CharSet=CharSet.Unicode, SetLastError=true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool ToSddl(
        IntPtr descriptor, uint revision, uint info, out IntPtr output, out uint length);
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern IntPtr LocalFree(IntPtr memory);
}
'@
Add-Type -TypeDefinition $native -ErrorAction Stop

$path = '\\.\pipe\openssh-ssh-agent'
$pipeHandle=[IntPtr]::Zero
$owner=[IntPtr]::Zero; $group=[IntPtr]::Zero; $dacl=[IntPtr]::Zero
$sacl=[IntPtr]::Zero; $descriptor=[IntPtr]::Zero; $sddlPointer=[IntPtr]::Zero
$pipeReport=[ordered]@{
    Pipe=$path; Sddl=$null; QueryResult=$null; AceList=@()
    CurrentUserSid=[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    BroadAccessDetected=$false; Limitations="An ACE alone does not establish effective access. Validate using a separate unprivileged account."
}
try {
    # Named pipes require a HANDLE and GetSecurityInfo, not GetNamedSecurityInfo.
    # READ_CONTROL=0x20000; FILE_SHARE_READ|FILE_SHARE_WRITE=3; OPEN_EXISTING=3.
    # This briefly connects to the pipe but sends no agent protocol messages.
    $pipeHandle=[OkpPipeSecurity]::CreateFile(
        $path,0x00020000,3,[IntPtr]::Zero,3,0,[IntPtr]::Zero)
    if ($pipeHandle -eq [IntPtr](-1)) {
        $last=[Runtime.InteropServices.Marshal]::GetLastWin32Error()
        $pipeReport.QueryResult=$last
        $pipeReport.QueryError="CreateFile for READ_CONTROL failed: Win32 code $last. The pipe may be busy or security policy denied access."
    } else {
        # SE_FILE_OBJECT=1; OWNER|GROUP|DACL information=7.
        $result=[OkpPipeSecurity]::GetSecurityInfo(
            $pipeHandle,1,7,[ref]$owner,[ref]$group,[ref]$dacl,[ref]$sacl,[ref]$descriptor)
        $pipeReport.QueryResult=$result
        if ($result -eq 0 -and $descriptor -ne [IntPtr]::Zero) {
        $len=[uint32]0
        if ([OkpPipeSecurity]::ToSddl($descriptor,1,7,[ref]$sddlPointer,[ref]$len)) {
            $sddl=[Runtime.InteropServices.Marshal]::PtrToStringUni($sddlPointer)
            $pipeReport.Sddl=$sddl
            $sd=New-Object System.Security.AccessControl.RawSecurityDescriptor($sddl)
            $broad=@("S-1-1-0","S-1-5-7","S-1-5-11","S-1-5-32-545")
            foreach($ace in $sd.DiscretionaryAcl) {
                $sid=$ace.SecurityIdentifier.Value
                $entry=[pscustomobject]@{
                    SID=$sid; Type=$ace.AceType.ToString()
                    Mask=("0x{0:X8}" -f ([uint32]$ace.AccessMask))
                }
                $pipeReport.AceList+= $entry
                if ($sid -in $broad -and $ace.AceType.ToString() -match "Allowed") {
                    $pipeReport.BroadAccessDetected=$true
                }
            }
        } else {
            $pipeReport.QueryError="Could not convert pipe security descriptor to SDDL."
        }
        } else {
            $pipeReport.QueryError="GetSecurityInfo returned Win32 code $result."
        }
    }
} finally {
    if ($pipeHandle -ne [IntPtr]::Zero -and $pipeHandle -ne [IntPtr](-1)) {
        [void][OkpPipeSecurity]::CloseHandle($pipeHandle)
    }
    if ($sddlPointer -ne [IntPtr]::Zero) { [void][OkpPipeSecurity]::LocalFree($sddlPointer) }
    if ($descriptor -ne [IntPtr]::Zero) { [void][OkpPipeSecurity]::LocalFree($descriptor) }
}
$pipeReport | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 (Join-Path $OutputDirectory "pipe-acl.json")

# Follow the OneKeePass process tree, including spawned msedgewebview2.exe.
# Record TCP only when discovered by sampling, and UDP *endpoints* (not sends).
$root=Get-CimInstance Win32_Process -Filter "Name='OneKeePass.exe'" |
    Sort-Object CreationDate -Descending | Select-Object -First 1
if (-not $root) { throw "Start OneKeePass.exe before running the audit. Pipe report saved in $OutputDirectory" }
$tracked=@{}
$tracked[[int]$root.ProcessId]="OneKeePass.exe"
$observations=New-Object 'System.Collections.Generic.List[object]'
$seen=@{}
$end=(Get-Date).AddSeconds($Seconds)
Write-Host "Monitoring OKP and WebView2 descendants for $Seconds seconds."
Write-Host "During monitoring: unlock/save/lock a test database and exercise SSH Agent."
while ((Get-Date) -lt $end) {
    $processes=@(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue)
    $updated=$true
    while($updated) {
        $updated=$false
        foreach($p in $processes) {
            $pidNumber=[int]$p.ProcessId
            if ($tracked.ContainsKey([int]$p.ParentProcessId) -and -not $tracked.ContainsKey($pidNumber)) {
                $tracked[$pidNumber]=$p.Name
                $updated=$true
            }
        }
    }
    $when=(Get-Date).ToString("o")
    foreach($tcp in @(Get-NetTCPConnection -ErrorAction SilentlyContinue)) {
        if (-not $tracked.ContainsKey([int]$tcp.OwningProcess)) { continue }
        $key="TCP|$($tcp.OwningProcess)|$($tcp.LocalAddress)|$($tcp.LocalPort)|$($tcp.RemoteAddress)|$($tcp.RemotePort)|$($tcp.State)"
        if ($seen.ContainsKey($key)) { continue }
        $seen[$key]=$true
        $observations.Add([pscustomobject]@{
            FirstObserved=$when; Protocol="TCP"; Process=$tracked[[int]$tcp.OwningProcess]
            PID=$tcp.OwningProcess; LocalAddress=$tcp.LocalAddress; LocalPort=$tcp.LocalPort
            RemoteAddress=$tcp.RemoteAddress; RemotePort=$tcp.RemotePort; State=$tcp.State
        })
    }
    Start-Sleep -Seconds $SampleIntervalSeconds
}
$observations | Export-Csv -NoTypeInformation -Encoding UTF8 (Join-Path $OutputDirectory "tcp-samples.csv")
$tracked.GetEnumerator() | ForEach-Object {
    [pscustomobject]@{PID=$_.Key; Process=$_.Value}
} | Export-Csv -NoTypeInformation -Encoding UTF8 (Join-Path $OutputDirectory "processes.csv")
$external=@($observations | Where-Object {
    $_.RemoteAddress -and $_.RemoteAddress -notin @("0.0.0.0","::","127.0.0.1","::1") -and
    $_.RemoteAddress -notlike "127.*" -and $_.State -notin @("Listen","Bound")
})
$summary=[ordered]@{
    AuditTime=(Get-Date).ToString("o")
    Seconds=$Seconds; IntervalSeconds=$SampleIntervalSeconds
    RootPID=$root.ProcessId; ObservedProcesses=$tracked.Count
    DistinctTCPRecords=$observations.Count
    NonLoopbackTCPRecords=$external.Count
    PipeQueryWin32Result=$pipeReport.QueryResult
    PipeBroadAllowAceFound=$pipeReport.BroadAccessDetected
    NetworkConclusion="INCONCLUSIVE: TCP polling misses transient connections and UDP datagrams, and child-process coverage is best effort. A WFP/ETW packet/process trace is required to verify absence of outbound traffic."
    PipeConclusion="INCONCLUSIVE until DACL and a separate-user negative-access test are reviewed."
    PrivacyNotice="Reports contain process identifiers, local Windows SID and network endpoints. Review before sharing."
}
$summary | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 (Join-Path $OutputDirectory "summary.json")
Write-Host "Reports saved to: $OutputDirectory"
Write-Host "Distinct TCP records: $($observations.Count); non-loopback records: $($external.Count)"
Write-Host "Named-pipe security query Win32 result: $($pipeReport.QueryResult)"
Write-Host "Broad allow ACE detected: $($pipeReport.BroadAccessDetected)"
Write-Warning "This is not a packet-level no-network proof. Review summary.json before treating the audit as passed."
