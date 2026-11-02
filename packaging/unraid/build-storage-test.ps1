$ErrorActionPreference='Stop'
. "$PSScriptRoot/build-storage.ps1" -Mode Definitions
function Assert($Condition,[string]$Reason){if(-not $Condition){throw $Reason}}
$snapshot=[pscustomobject]@{CacheBytes=50GB;LimitBytes=120GB;SystemFreeBytes=70GB;SystemReserveBytes=30GB;CacheDiskFreeBytes=300GB;CacheReserveBytes=80GB}
Assert ($null -eq (Get-StorageViolation $snapshot)) 'Normal build should proceed'
$snapshot|Add-Member -NotePropertyName CachePhysicalBytes -NotePropertyValue 160GB
$snapshot|Add-Member -NotePropertyName PhysicalLimitBytes -NotePropertyValue 160GB
Assert ((Get-StorageViolation $snapshot) -like '*physical size*') 'Native disk growth must also be bounded'
$snapshot.CachePhysicalBytes=0
$snapshot.CacheBytes=120GB
Assert ((Get-StorageViolation $snapshot) -like '*size limit*') 'Exact cache limit must stop the build'
$snapshot.CacheBytes=50GB;$snapshot.SystemFreeBytes=29GB
Assert ((Get-StorageViolation $snapshot) -like '*System*') 'System reserve must be enforced'
$snapshot.SystemFreeBytes=70GB;$snapshot.CacheDiskFreeBytes=79GB
Assert ((Get-StorageViolation $snapshot) -like '*cache disk*') 'Cache disk reserve must be enforced'
$settings=[pscustomobject]@{ImageId='sha256:public-test';Workspace='Q:\public-source';Target='Q:\public-cache\target';Registry='Q:\public-cache\registry'}
$container=[pscustomobject]@{Name='/solidcraft-test';Image='sha256:public-test';Labels=@{'solidcraft.task'='storage-proof'};Mounts=@(
 @{Destination='/src';Type='bind';Source='/run/desktop/mnt/host/q/public-source'},
 @{Destination='/src/target';Type='bind';Source='/run/desktop/mnt/host/q/public-cache/target'},
 @{Destination='/usr/local/cargo/registry';Type='bind';Source='/run/desktop/mnt/host/q/public-cache/registry'})}
Assert (Test-OwnedBuild $container $settings 'solidcraft-test' 'storage-proof') 'Recognize only the expected build'
$container.Mounts[2].Source='/run/desktop/mnt/host/q/foreign-registry'
Assert (-not(Test-OwnedBuild $container $settings 'solidcraft-test' 'storage-proof')) 'A different registry mount must never be stopped'
$container.Mounts[2].Source='/run/desktop/mnt/host/q/public-cache/registry'
$container.Labels.'solidcraft.task'='another-task'
Assert (-not(Test-OwnedBuild $container $settings 'solidcraft-test' 'storage-proof')) 'A different task must never be stopped'
$container.Labels.'solidcraft.task'='storage-proof';$container.Mounts[1].Source='/run/desktop/mnt/host/q/private-project-data'
Assert (-not(Test-OwnedBuild $container $settings 'solidcraft-test' 'storage-proof')) 'A different mount must never be stopped'
$container.Mounts[1].Source='/run/desktop/mnt/host/q/public-cache/target';$container.Image='sha256:another-image'
Assert (-not(Test-OwnedBuild $container $settings 'solidcraft-test' 'storage-proof')) 'A different image must never be stopped'
$testRoot=Join-Path ([IO.Path]::GetTempPath()) ('solidcraft-storage-test-'+[guid]::NewGuid().ToString('N'))
$outside=Join-Path ([IO.Path]::GetTempPath()) ('solidcraft-storage-outside-'+[guid]::NewGuid().ToString('N'))
try {
 $null=New-Item -ItemType Directory -Path "$testRoot/nested",$outside
 [IO.File]::WriteAllBytes("$testRoot/nested/data",[byte[]]::new(4096))
 [IO.File]::WriteAllBytes("$outside/preserved",[byte[]]::new(2048))
 $null=New-Item -ItemType Junction -Path "$testRoot/external" -Target $outside
 Assert ((Get-CacheBytes $testRoot) -eq 4096) 'Measurement must not follow a junction outside the cache'
 Assert ((Get-Item -LiteralPath "$outside/preserved").Length -eq 2048) 'An outside file must be untouched'
} finally {
 foreach($path in @($testRoot,$outside)) {
  $resolved=[IO.Path]::GetFullPath($path)
  $temp=[IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\','/')+[IO.Path]::DirectorySeparatorChar
  if(-not $resolved.StartsWith($temp,[StringComparison]::OrdinalIgnoreCase)){throw 'Temporary cleanup path outside test root'}
 }
 if(Test-Path -LiteralPath "$testRoot/external"){[IO.Directory]::Delete("$testRoot/external")}
 Remove-Item -LiteralPath $testRoot,$outside -Recurse -Force -ErrorAction SilentlyContinue
}
'PASS: cache limits, both disk reserves, strict build ownership and junction exclusion'
