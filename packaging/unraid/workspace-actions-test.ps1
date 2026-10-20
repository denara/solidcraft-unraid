param(
    [string]$BaseUrl = 'http://localhost:8781',
    [Parameter(Mandatory=$true)][string]$Token,
    [string]$ContainerName = 'proof',
    [string]$OutputDirectory = 'target/unraid-workspace-actions-proof'
)
# Destructive tests are confined to new, uniquely named files in a labelled proof container.
$ErrorActionPreference='Stop'
$BaseUrl=$BaseUrl.TrimEnd('/')
$taskInspect=(docker inspect $ContainerName | ConvertFrom-Json)[0]
if($LASTEXITCODE -ne 0 -or $taskInspect.Config.Labels.'solidcraft.proof' -ne 'true'){throw 'Refusing file-action tests: container must carry solidcraft.proof=true and use a disposable volume'}
$taskUri=[uri]$BaseUrl
$taskBindings=@($taskInspect.NetworkSettings.Ports.'8080/tcp' | Where-Object { $_.HostIp -in @('127.0.0.1','::1') -and [int]$_.HostPort -eq $taskUri.Port })
if($taskUri.Scheme -ne 'http' -or $taskUri.Host -notin @('localhost','127.0.0.1','[::1]','::1') -or $taskBindings.Count -ne 1){throw 'BaseUrl must address the inspected disposable container through its loopback port'}
$taskProject='actions-proof-'+[guid]::NewGuid().ToString('N')
$taskId=0
$taskAuth=@{Authorization="Bearer $Token";Accept='application/json, text/event-stream';'MCP-Protocol-Version'='2025-06-18'}
function Rpc([string]$Method,[hashtable]$Params=@{},[switch]$AllowError) {
 $script:taskId++
 $body=@{jsonrpc='2.0';id=$script:taskId;method=$Method;params=$Params}|ConvertTo-Json -Depth 20 -Compress
 $response=Invoke-WebRequest "$BaseUrl/mcp" -Method Post -Headers $taskAuth -ContentType application/json -Body $body
 if($Method -eq 'initialize'){$taskAuth['Mcp-Session-Id']=[string]@($response.Headers['Mcp-Session-Id'])[0]}
 $result=($response.Content|ConvertFrom-Json -Depth 30).result
 if($result.isError -and -not $AllowError){throw 'MCP file-action fixture failed'}
 $result
}
function Tool([string]$Name,[hashtable]$Arguments=@{}) {Rpc 'tools/call' @{name=$Name;arguments=$Arguments}}
function Preview([string]$Name) {
 $response=Invoke-WebRequest "$BaseUrl/workspace/delete/$Name" -WebSession $taskBrowser -Headers @{Origin=$BaseUrl}
 $ticket=[regex]::Match($response.Content,'name="ticket" value="([^"]+)"').Groups[1].Value
 $csrf=[regex]::Match($response.Content,'name="csrf" value="([^"]+)"').Groups[1].Value
 if(-not $ticket -or -not $csrf){throw 'Delete confirmation is missing'}
 @{ticket=$ticket;csrf=$csrf;path=$Name;confirm='delete'}
}
function Delete([hashtable]$Form,[hashtable]$Headers=@{Origin=$BaseUrl}) {
 Invoke-WebRequest "$BaseUrl/workspace/delete" -Method Post -WebSession $taskBrowser -Headers $Headers -Body $Form -SkipHttpErrorCheck
}
function Rss {
 $line=docker exec $ContainerName sh -c 'for p in /proc/[0-9]*; do read c < "$p/comm" || continue; case "$c" in solvecraft-ser*) grep VmRSS "$p/status";; esac; done'
 if($LASTEXITCODE -ne 0){throw 'Cannot measure server memory'}
 $value=[regex]::Match(($line -join ' '),'VmRSS:\s*(\d+)').Groups[1].Value
 if(-not $value){throw 'No server RSS measurement'}
 [long]$value*1024
}
New-Item -ItemType Directory -Force $OutputDirectory|Out-Null
$null=Rpc initialize @{protocolVersion='2025-06-18';capabilities=@{};clientInfo=@{name='workspace-file-actions-proof';version='1'}}
try {
 $null=Tool project_select @{name=$taskProject}
 # All paths below use a newly selected, random project in this disposable candidate.
 $scriptText='set -eu; root="/data/files/$1"; test -d "$root"; test ! -e "$root/nested"; mkdir -p "$root/nested/empty"; printf public > "$root/nested/notes.txt"; printf unicode > "$root/nested/Grüße.txt"; : > "$root/nested/zero.txt"; dd if=/dev/zero of="$root/nested/large.bin" bs=1048576 count=256 status=none; printf root > "$root/single.txt"'
 $null=docker exec $ContainerName sh -c $scriptText -- $taskProject
 if($LASTEXITCODE -ne 0){throw 'Disposable file fixtures failed'}
 $null=Invoke-WebRequest "$BaseUrl/workspace/login" -SessionVariable taskBrowser
 $login=Invoke-WebRequest "$BaseUrl/workspace/login" -WebSession $taskBrowser
 $csrf=[regex]::Match($login.Content,'name="csrf" value="([^"]+)"').Groups[1].Value
 $null=Invoke-WebRequest "$BaseUrl/workspace/login" -Method Post -WebSession $taskBrowser -Headers @{Origin=$BaseUrl} -Body @{csrf=$csrf;token=$Token}
 $taskBrowser.Headers.Clear()
 if((Invoke-WebRequest "$BaseUrl/workspace/delete/$taskProject/single.txt" -WebSession $taskBrowser -SkipHttpErrorCheck).StatusCode -ne 409){throw 'Active MCP project was not protected'}
 $detail=(Tool workspace_list_files @{path="$taskProject/single.txt"}).structuredContent.files[0]
 $null=Invoke-WebRequest "$BaseUrl/mcp" -Method Delete -Headers $taskAuth
 $taskAuth.Remove('Mcp-Session-Id')
 $form=Preview "$taskProject/single.txt"
 $bad=$form.Clone();$bad.csrf='bad'
 if((Delete $bad).StatusCode -ne 403){throw 'CSRF bypass'}
 if((Delete $form @{Origin='https://attacker.example'}).StatusCode -ne 403){throw 'Origin bypass'}
 $taskBrowser.Headers.Clear()
 # A matching conditional upload changes the confirmation's strong revision.
 $null=Invoke-WebRequest "$BaseUrl/files/$taskProject/single.txt" -Method Put -Headers @{Authorization="Bearer $Token";'If-Match'=$detail.revision} -Body 'new content'
 if((Delete $form).StatusCode -ne 409){throw 'Stale delete confirmation accepted'}
 $form=Preview "$taskProject/single.txt"
 $null=Delete $form
 if((Invoke-WebRequest "$BaseUrl/files/$taskProject/single.txt" -Headers @{Authorization="Bearer $Token"} -SkipHttpErrorCheck).StatusCode -ne 404){throw 'File not deleted'}
 # Native streaming consumer: headers and first bytes arrive before the archive exists.
 $handler=[Net.Http.HttpClientHandler]::new();$handler.CookieContainer=$taskBrowser.Cookies
 $client=[Net.Http.HttpClient]::new($handler);$client.Timeout=[TimeSpan]::FromMinutes(4)
 $before=Rss
 $timer=[Diagnostics.Stopwatch]::StartNew()
 $response=$client.GetAsync("$BaseUrl/workspace/archive/$taskProject/nested",[Net.Http.HttpCompletionOption]::ResponseHeadersRead).GetAwaiter().GetResult()
 if([int]$response.StatusCode -ne 200){throw 'ZIP stream rejected'}
 $stream=$response.Content.ReadAsStreamAsync().GetAwaiter().GetResult()
 $buffer=[byte[]]::new(32768);$count=$stream.Read($buffer,0,$buffer.Length)
 if($count -lt 4 -or $buffer[0] -ne 80 -or $buffer[1] -ne 75){throw 'No early ZIP bytes'}
 $earlyMs=$timer.ElapsedMilliseconds
 Start-Sleep -Milliseconds 400
 $peak=[Math]::Max($before,(Rss))
 $blocked=Invoke-WebRequest "$BaseUrl/files/$taskProject/nested/new.txt" -Method Put -Headers @{Authorization="Bearer $Token"} -Body 'must be blocked' -SkipHttpErrorCheck
 if($blocked.StatusCode -ne 409){throw 'ZIP path lease did not protect concurrent writes'}
 if((Invoke-WebRequest "$BaseUrl/workspace/delete/$taskProject/nested" -WebSession $taskBrowser -SkipHttpErrorCheck).StatusCode -ne 409){throw 'ZIP path lease did not prevent deletion'}
 $null=Rpc initialize @{protocolVersion='2025-06-18';capabilities=@{};clientInfo=@{name='zip-concurrency-proof';version='1'}}
 $null=Tool project_select @{name=$taskProject}
 $null=Tool execute @{command='solid.box';params=@{length=10;width=10;height=10}}
 $blockedMcp=Rpc 'tools/call' @{name='save';arguments=@{path='nested/blocked.solidcraft'}} -AllowError
 if(-not $blockedMcp.isError){throw 'ZIP lease did not reject a matching MCP save'}
 $null=Invoke-WebRequest "$BaseUrl/mcp" -Method Delete -Headers $taskAuth
 $taskAuth.Remove('Mcp-Session-Id')
 $otherProject="Other-$taskProject"
 $null=Rpc initialize @{protocolVersion='2025-06-18';capabilities=@{};clientInfo=@{name='zip-independent-project-proof';version='1'}}
 $null=Tool project_select @{name=$otherProject}
 $null=Tool execute @{command='solid.box';params=@{length=10;width=10;height=10}}
 $null=Tool save @{path='public.solidcraft'}
 $null=Invoke-WebRequest "$BaseUrl/mcp" -Method Delete -Headers $taskAuth
 $taskAuth.Remove('Mcp-Session-Id')
 $other="unrelated-$taskProject.txt"
 $null=Invoke-WebRequest "$BaseUrl/files/$other" -Method Put -Headers @{Authorization="Bearer $Token"} -Body 'unrelated write remains usable'
 $archive=Join-Path $OutputDirectory 'nested.zip'
 $output=[IO.File]::Create([IO.Path]::GetFullPath($archive));$output.Write($buffer,0,$count)
 $received=[long]$count;$nextSample=16MB
 try {while(($count=$stream.Read($buffer,0,$buffer.Length)) -gt 0){$output.Write($buffer,0,$count);$received+=$count;if($received -ge $nextSample){$peak=[Math]::Max($peak,(Rss));$nextSample+=16MB}}} finally {$output.Dispose();$stream.Dispose();$response.Dispose()}
 $peak=[Math]::Max($peak,(Rss))
 if($peak-$before -gt 48MB){throw 'ZIP RSS grew beyond the bounded-memory allowance'}
 $zip=[IO.Compression.ZipFile]::OpenRead([IO.Path]::GetFullPath($archive))
 try {
  if(-not $zip.GetEntry('nested/empty/') -or -not $zip.GetEntry('nested/Grüße.txt') -or $zip.GetEntry('nested/zero.txt').Length -ne 0 -or $zip.GetEntry('nested/large.bin').Length -ne 256MB){throw 'ZIP paths, Unicode, empties or large content missing'}
  $large=$zip.GetEntry('nested/large.bin').Open()
  try {$actual=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($large))} finally {$large.Dispose()}
  $zeros=[byte[]]::new(32768);$expected=[Security.Cryptography.IncrementalHash]::CreateHash([Security.Cryptography.HashAlgorithmName]::SHA256)
  try {for($i=0;$i -lt 8192;$i++){$expected.AppendData($zeros)};$wanted=[Convert]::ToHexString($expected.GetHashAndReset())} finally {$expected.Dispose()}
  if($actual -ne $wanted){throw 'ZIP large-file data changed or truncated'}
 } finally {$zip.Dispose()}
 # Cancellation releases the folder lease and stream slot without finishing an archive.
 $response=$client.GetAsync("$BaseUrl/workspace/archive/$taskProject/nested",[Net.Http.HttpCompletionOption]::ResponseHeadersRead).GetAwaiter().GetResult()
 $stream=$response.Content.ReadAsStreamAsync().GetAwaiter().GetResult();$null=$stream.Read($buffer,0,$buffer.Length);$stream.Dispose();$response.Dispose()
 $released=$false
 for($i=0;$i -lt 50;$i++) {
  $put=Invoke-WebRequest "$BaseUrl/files/$taskProject/nested/after-abort.txt" -Method Put -Headers @{Authorization="Bearer $Token"} -Body 'released' -SkipHttpErrorCheck
  if($put.StatusCode -eq 201){$released=$true;break};if($put.StatusCode -ne 409){throw 'Unexpected abort cleanup response'};Start-Sleep -Milliseconds 100
 }
 if(-not $released){throw 'Cancelled ZIP retained its lease'}
 $client.Dispose()
 $form=Preview "$taskProject/nested"
 $null=Delete $form
 $snapshot=Invoke-RestMethod "$BaseUrl/workspace/api" -WebSession $taskBrowser
 if(@($snapshot.files|Where-Object {$_.name.StartsWith("$taskProject/nested/")}).Count -or "$taskProject/nested" -in $snapshot.directories){throw 'Deleted folder remains in live inventory'}
 $form=Preview $taskProject;$null=Delete $form
 $form=Preview $otherProject;$null=Delete $form
 $form=Preview $other;$null=Delete $form
 @{result='pass';early_zip_ms=$earlyMs;archive_bytes=(Get-Item $archive).Length;uncompressed_large_bytes=256MB;rss_before_bytes=$before;rss_peak_bytes=$peak;rss_growth_bytes=($peak-$before);covered=@('cookie-CSRF-Origin','strong-delete-revision','active-project','real-file-and-folder-delete','ZIP64-Unicode-empty-nested-large','early-stream-backpressure','matching-MCP-rejection','independent-MCP-save','unrelated-write','abort-release','live-inventory')}|ConvertTo-Json -Depth 5|Set-Content (Join-Path $OutputDirectory 'proof.json') -Encoding utf8
 'PASS: confirmed deletion and bounded server-streamed folder ZIPs'
} finally {
 if($taskAuth['Mcp-Session-Id']){$null=Invoke-WebRequest "$BaseUrl/mcp" -Method Delete -Headers $taskAuth -SkipHttpErrorCheck}
}
