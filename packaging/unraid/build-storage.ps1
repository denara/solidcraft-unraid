# Local development only. Private paths belong in ignored target/build-storage.json.
[CmdletBinding()]
param(
 [ValidateSet('Check','Run','Definitions')][string]$Mode='Check',
 [string]$ConfigPath=(Join-Path $PSScriptRoot '../../target/build-storage.json'),
 [string]$Name,
 [string]$Task,
 [string]$ProofDirectory,
 [string]$Script,
 [string]$LogPath,
 [string]$NetworkContainer,
 [ValidateRange(1,12)][int]$MemoryGiB=12,
 [ValidateRange(1,4)][int]$Cpus=4,
 [ValidateRange(1,3)][int]$Jobs=2
)
$ErrorActionPreference='Stop'
function Get-CacheBytes([string]$Root) {
 $pending=[Collections.Generic.Stack[string]]::new();$pending.Push($Root)
 [long]$bytes=0
 while($pending.Count -gt 0) {
  $directory=$pending.Pop()
  try {$entries=[IO.Directory]::GetFileSystemEntries($directory)}
  catch [IO.DirectoryNotFoundException] {continue}
  foreach($entry in $entries) {
   try {
    $attributes=[IO.File]::GetAttributes($entry)
    if($attributes -band [IO.FileAttributes]::ReparsePoint){continue}
    if($attributes -band [IO.FileAttributes]::Directory){$pending.Push($entry)}
    else {$bytes+=[IO.FileInfo]::new($entry).Length}
   } catch [IO.FileNotFoundException] {continue}
     catch [IO.DirectoryNotFoundException] {continue}
  }
 }
 return $bytes
}
function Get-StorageViolation($Snapshot) {
 if($Snapshot.CachePhysicalBytes -and $Snapshot.CachePhysicalBytes -ge $Snapshot.PhysicalLimitBytes){return 'Build-cache disk image reached its physical size ceiling'}
 if($Snapshot.CacheBytes -ge $Snapshot.LimitBytes){return 'SolidCraft build cache reached its size limit'}
 if($Snapshot.SystemFreeBytes -lt $Snapshot.SystemReserveBytes){return 'System disk reserve is too low'}
 if($Snapshot.CacheDiskFreeBytes -lt $Snapshot.CacheReserveBytes){return 'Build-cache disk reserve is too low'}
 return $null
}
function Get-CanonicalHostPath([string]$Value) {
 $path=$Value.Replace('\','/').TrimEnd('/').ToLowerInvariant()
 if($path -match '^/run/desktop/mnt/host/([a-z])/(.*)$'){return "$($Matches[1]):/$($Matches[2])"}
 return $path
}
function Test-OwnedBuild($Container,$Settings,[string]$ExpectedName,[string]$ExpectedTask) {
 if($Container.Name -cne "/$ExpectedName" -or $Container.Image -cne $Settings.ImageId -or
    $Container.Labels.'solidcraft.task' -cne $ExpectedTask){return $false}
 $source=@($Container.Mounts|Where-Object Destination -eq '/src')
 $cache=@($Container.Mounts|Where-Object Destination -eq '/src/target')
 $registry=@($Container.Mounts|Where-Object Destination -eq '/usr/local/cargo/registry')
 if($source.Count -ne 1 -or $cache.Count -ne 1 -or $registry.Count -ne 1 -or -not $source[0].Source -or -not $cache[0].Source -or -not $registry[0].Source){return $false}
 return $source.Count -eq 1 -and $cache.Count -eq 1 -and
  $source[0].Type -eq 'bind' -and $cache[0].Type -eq 'bind' -and $registry[0].Type -eq 'bind' -and
  (Get-CanonicalHostPath $source[0].Source) -ceq (Get-CanonicalHostPath $Settings.Workspace) -and
  (Get-CanonicalHostPath $cache[0].Source) -ceq (Get-CanonicalHostPath $Settings.Target) -and
  (Get-CanonicalHostPath $registry[0].Source) -ceq (Get-CanonicalHostPath $Settings.Registry)
}
function Invoke-StorageDocker([string[]]$Arguments,[int]$TimeoutSeconds=12) {
 $info=[Diagnostics.ProcessStartInfo]::new('docker')
 $info.UseShellExecute=$false;$info.CreateNoWindow=$true
 $info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
 foreach($argument in $Arguments){$info.ArgumentList.Add($argument)}
 $process=[Diagnostics.Process]::new();$process.StartInfo=$info
 if(-not $process.Start()){throw 'Docker command did not start'}
 try {
  $output=$process.StandardOutput.ReadToEndAsync();$errorOutput=$process.StandardError.ReadToEndAsync()
  if(-not $process.WaitForExit($TimeoutSeconds*1000)){$process.Kill();throw 'Docker command timed out'}
  if($process.ExitCode -ne 0){throw "Docker command failed: $($errorOutput.GetAwaiter().GetResult())"}
  return $output.GetAwaiter().GetResult()
 } finally {$process.Dispose()}
}
function Invoke-CacheWsl($Settings,[string[]]$Arguments) {
 $info=[Diagnostics.ProcessStartInfo]::new('wsl.exe')
 $info.UseShellExecute=$false;$info.CreateNoWindow=$true
 $info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
 foreach($argument in (@('-d',$Settings.WslDistribution,'-u','root','--exec')+$Arguments)){$info.ArgumentList.Add($argument)}
 $process=[Diagnostics.Process]::new();$process.StartInfo=$info
 if(-not $process.Start()){throw 'Cache filesystem command did not start'}
 try {
  $output=$process.StandardOutput.ReadToEndAsync();$errorOutput=$process.StandardError.ReadToEndAsync()
  if(-not $process.WaitForExit(30000)){$process.Kill();throw 'Cache filesystem command timed out'}
  if($process.ExitCode -ne 0){throw "Cache filesystem command failed: $($errorOutput.GetAwaiter().GetResult())"}
  return $output.GetAwaiter().GetResult()
 } finally {$process.Dispose()}
}
function Read-StorageSettings([string]$Path) {
 $settings=Get-Content -LiteralPath $Path -Raw|ConvertFrom-Json
 if($settings.Schema -ne 1 -or $settings.LimitGiB -lt 1 -or $settings.LimitGiB -gt 120 -or
    $settings.SystemReserveGiB -lt 30 -or $settings.CacheReserveGiB -lt 80){throw 'Invalid build-storage limits'}
 foreach($property in @('Root','Workspace')) {
  $settings.$property=[IO.Path]::GetFullPath($settings.$property)
  if(-not(Test-Path -LiteralPath $settings.$property -PathType Container)){throw "Missing storage directory: $property"}
  if((Get-Item -LiteralPath $settings.$property).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Storage roots must not be links or junctions'}
 }
 $root=$settings.Root.TrimEnd([IO.Path]::DirectorySeparatorChar,[IO.Path]::AltDirectorySeparatorChar)+[IO.Path]::DirectorySeparatorChar
 $systemRoot=[IO.Path]::GetPathRoot([Environment]::SystemDirectory)
 if([IO.Path]::GetPathRoot($settings.Root) -ieq $systemRoot){throw 'Build cache must be off the system disk'}
 if($settings.Root -ceq [IO.Path]::GetPathRoot($settings.Root)){throw 'A whole drive cannot be a build cache'}
 if($settings.Workspace.StartsWith($root,[StringComparison]::OrdinalIgnoreCase) -or $settings.Workspace -ieq $settings.Root){throw 'The workspace cannot be stored in the disposable cache'}
 $owner=Get-Content -LiteralPath (Join-Path $settings.Root '.solidcraft-build-cache.json') -Raw|ConvertFrom-Json
 if($owner.Owner -cne 'denara/solidcraft-unraid' -or $owner.Purpose -cne 'disposable-build-cache'){throw 'Build-cache ownership marker differs'}
 if($settings.Backend -eq 'wsl') {
  if($settings.WslDistribution -cne 'SolidCraftBuildCache' -or $settings.Target -cne '/run/desktop/mnt/host/wsl/solidcraft-build-cache/target' -or
     $settings.Registry -cne '/run/desktop/mnt/host/wsl/solidcraft-build-cache/registry'){throw 'Native cache identity differs'}
  $settings.WslDisk=[IO.Path]::GetFullPath($settings.WslDisk)
  if(-not $settings.WslDisk.StartsWith($root,[StringComparison]::OrdinalIgnoreCase) -or -not(Test-Path -LiteralPath $settings.WslDisk -PathType Leaf)){throw 'Native cache disk is outside its marked root'}
  $registrations=@(Get-ChildItem HKCU:/Software/Microsoft/Windows/CurrentVersion/Lxss|ForEach-Object {Get-ItemProperty -LiteralPath $_.PSPath}|Where-Object DistributionName -CEQ $settings.WslDistribution)
  if($registrations.Count -ne 1){throw 'Native cache filesystem registration differs'}
  $base=([string]$registrations[0].BasePath) -replace '^\x5c\x5c\?\x5c',''
  if([IO.Path]::GetFullPath($base).TrimEnd([IO.Path]::DirectorySeparatorChar) -ine [IO.Path]::GetDirectoryName($settings.WslDisk) -or
     [IO.Path]::GetFileName($settings.WslDisk) -ine 'ext4.vhdx'){throw 'Registered cache disk is not on its configured host path'}
  if($settings.PhysicalLimitGiB -lt ($settings.LimitGiB+16) -or $settings.PhysicalLimitGiB -gt 160){throw 'Native cache physical ceiling differs'}
 } else {
  foreach($property in @('Target','Registry')) {
   $settings.$property=[IO.Path]::GetFullPath($settings.$property)
   if(-not $settings.$property.StartsWith($root,[StringComparison]::OrdinalIgnoreCase) -or -not(Test-Path -LiteralPath $settings.$property -PathType Container)){throw 'Cache directory is outside its marked root'}
   if((Get-Item -LiteralPath $settings.$property).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Cache directories must not be links'}
  }
 }
 if($settings.ImageId -notmatch '^sha256:[0-9a-f]{64}$'){throw 'An immutable toolchain image ID is required'}
 return $settings
}
function Ensure-NativeCache($Settings) {
 if($Settings.Backend -ne 'wsl'){return}
 $ready=Invoke-CacheWsl $Settings @('/usr/local/bin/solidcraft-cache-mount')
 if($ready.Trim() -cne 'CACHE_READY'){throw 'Native cache mount was not verified'}
 $alive=@(Get-CimInstance Win32_Process -Filter "Name='wsl.exe'"|Where-Object {$_.CommandLine -like '*-d SolidCraftBuildCache*--exec /bin/sleep 2147483647*'})
 if($alive.Count -eq 0) {
  $process=Start-Process -FilePath wsl.exe -ArgumentList @('-d','SolidCraftBuildCache','-u','root','--exec','/bin/sleep','2147483647') -WindowStyle Hidden -PassThru
  @{pid=$process.Id;distribution=$Settings.WslDistribution}|ConvertTo-Json|Set-Content (Join-Path $Settings.Root 'keepalive.json') -Encoding utf8
 }
}
function Get-StorageSnapshot($Settings,[Nullable[long]]$CacheBytes=$null) {
 [long]$physical=0
 if($Settings.Backend -eq 'wsl') {
  $physical=Get-CacheBytes $Settings.Root
  if($null -eq $CacheBytes) {
   $usage=Invoke-CacheWsl $Settings @('/usr/bin/du','-sk','/cache')
   if($usage -notmatch '^([0-9]+)\s'){throw 'Native cache size is unavailable'}
   $CacheBytes=[long]$Matches[1]*1024
  }
 } elseif($null -eq $CacheBytes){$CacheBytes=Get-CacheBytes $Settings.Root}
 return [pscustomobject]@{
  CacheBytes=[long]$CacheBytes;LimitBytes=[long]($Settings.LimitGiB*1GB)
  CachePhysicalBytes=$physical;PhysicalLimitBytes=[long]($Settings.PhysicalLimitGiB*1GB)
  SystemFreeBytes=[IO.DriveInfo]::new([IO.Path]::GetPathRoot([Environment]::SystemDirectory)).AvailableFreeSpace
  SystemReserveBytes=[long]($Settings.SystemReserveGiB*1GB)
  CacheDiskFreeBytes=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($Settings.Root)).AvailableFreeSpace
  CacheReserveBytes=[long]($Settings.CacheReserveGiB*1GB)
 }
}
if($Mode -eq 'Definitions'){return}
$settings=Read-StorageSettings $ConfigPath
$snapshot=Get-StorageSnapshot $settings
$violation=Get-StorageViolation $snapshot
if($violation){throw $violation}
Ensure-NativeCache $settings
if($Mode -eq 'Check'){$snapshot|ConvertTo-Json;return}
if($Name -notmatch '^solidcraft-[a-z0-9-]{1,100}$' -or $Task -notmatch '^[a-z0-9-]{1,100}$' -or $Script -notmatch '^[a-z0-9-]+\.sh$'){throw 'Invalid build identity or script'}
$proof=[IO.Path]::GetFullPath($ProofDirectory)
$proofRoot=[IO.Path]::GetFullPath((Join-Path $settings.Workspace 'target')).TrimEnd('\','/')+[IO.Path]::DirectorySeparatorChar
if(-not $proof.StartsWith($proofRoot,[StringComparison]::OrdinalIgnoreCase) -or -not(Test-Path -LiteralPath (Join-Path $proof $Script))){throw 'Build scripts must be in the private target evidence directory'}
$log=[IO.Path]::GetFullPath($LogPath)
if(-not $log.StartsWith($proof+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Build logs must remain in its evidence directory'}
$network=@()
if($NetworkContainer) {
 if($NetworkContainer -notmatch '^solidcraft-[a-z0-9-]{1,100}$'){throw 'Invalid isolated network peer'}
 $peer=Invoke-StorageDocker @('inspect','--format','{"Name":{{json .Name}},"Labels":{{json .Config.Labels}},"Mounts":{{json .Mounts}},"Running":{{json .State.Running}}}',$NetworkContainer)|ConvertFrom-Json
 $data=@($peer.Mounts|Where-Object Destination -eq '/data')
 if($peer.Name -cne "/$NetworkContainer" -or -not $peer.Running -or $peer.Labels.'solidcraft.task' -cne $Task -or
    $peer.Labels.'solidcraft.proof' -cne 'true' -or $data.Count -ne 1 -or $data[0].Type -cne 'volume'){throw 'Network peer must be a running isolated candidate with a test data volume'}
 $volumeLabels=Invoke-StorageDocker @('volume','inspect','--format','{{json .Labels}}',$data[0].Name)|ConvertFrom-Json
 if($volumeLabels.'solidcraft.task' -cne $Task){throw 'Candidate test-volume ownership differs'}
 $network=@('--network',"container:$NetworkContainer")
}
$arguments=@('run')+$network+@('--name',$Name,'--label',"solidcraft.task=$Task",'--memory',"${MemoryGiB}g",'--cpus',"$Cpus",'--pids-limit','512',
 '-v',"$($settings.Workspace):/src",'-v',"$($settings.Target):/src/target",'-v',"$($settings.Registry):/usr/local/cargo/registry",'-v',"${proof}:/proof",'-w','/src','-e',"CARGO_BUILD_JOBS=$Jobs",'-e','CARGO_INCREMENTAL=0',$settings.ImageId,'sh',"/proof/$Script")
$info=[Diagnostics.ProcessStartInfo]::new('docker');$info.UseShellExecute=$false;$info.CreateNoWindow=$true
$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
foreach($argument in $arguments){$info.ArgumentList.Add($argument)}
$process=[Diagnostics.Process]::new();$process.StartInfo=$info
$stdout=[IO.File]::Open($log,[IO.FileMode]::Create,[IO.FileAccess]::Write,[IO.FileShare]::Read);$stderr=[IO.File]::Open("$log.stderr",[IO.FileMode]::Create,[IO.FileAccess]::Write,[IO.FileShare]::Read)
$owned=$null;$started=$false;$nextMeasure=[DateTime]::UtcNow.AddSeconds(30)
try {
 if(-not $process.Start()){throw 'Build command did not start'}
 $started=$true
 $outCopy=$process.StandardOutput.BaseStream.CopyToAsync($stdout);$errCopy=$process.StandardError.BaseStream.CopyToAsync($stderr)
 while(-not $process.WaitForExit(3000)) {
  if([DateTime]::UtcNow -ge $nextMeasure){$snapshot=Get-StorageSnapshot $settings;$nextMeasure=[DateTime]::UtcNow.AddSeconds(30)}
  else {$snapshot=Get-StorageSnapshot $settings $snapshot.CacheBytes}
  $violation=Get-StorageViolation $snapshot
  if($violation) {
   $format='{"Name":{{json .Name}},"Image":{{json .Image}},"Labels":{{json .Config.Labels}},"Mounts":{{json .Mounts}}}'
   $owned=Invoke-StorageDocker @('inspect','--format',$format,$Name)|ConvertFrom-Json
   if(-not(Test-OwnedBuild $owned $settings $Name $Task)){throw 'Storage limit reached; container ownership could not be verified'}
   $null=Invoke-StorageDocker @('stop','--time','10',$Name) 20
   @{reason=$violation;container=$Name;snapshot=$snapshot}|ConvertTo-Json -Depth 5|Set-Content "$log.storage-limit.json" -Encoding utf8
   throw "$violation; stopped only the verified SolidCraft build container"
  }
 }
 $null=$outCopy.GetAwaiter().GetResult();$null=$errCopy.GetAwaiter().GetResult()
 if($process.ExitCode -ne 0){throw "Build exited with code $($process.ExitCode); see its preserved logs"}
 $final=Get-StorageSnapshot $settings
 if(Get-StorageViolation $final){throw 'Build completed but storage limits require cleanup before another build'}
 $final|ConvertTo-Json
} catch {
 $failure=$_
 if($started -and -not $process.HasExited) {
  try {
   $format='{"Name":{{json .Name}},"Image":{{json .Image}},"Labels":{{json .Config.Labels}},"Mounts":{{json .Mounts}}}'
   $owned=Invoke-StorageDocker @('inspect','--format',$format,$Name)|ConvertFrom-Json
   if(Test-OwnedBuild $owned $settings $Name $Task){$null=Invoke-StorageDocker @('stop','--time','10',$Name) 20}
   else {Write-Warning 'Build ownership could not be verified; no other container was stopped'}
  } catch {Write-Warning 'Docker did not confirm the owned build stop; retain diagnostics and inspect its state'}
  $null=$process.WaitForExit(20000)
 }
 throw $failure
} finally {
 $stdout.Dispose();$stderr.Dispose();$process.Dispose()
}
