param(
    [string]$BaseUrl = 'http://localhost:8781',
    [Parameter(Mandatory = $true)][string]$Token,
    [string]$OutputDirectory = 'target/unraid-workspace-proof'
)
# Only use a disposable candidate: this proof creates its own public example files.
$ErrorActionPreference = 'Stop'
$BaseUrl = $BaseUrl.TrimEnd('/')
$taskId = 0
$taskSessions = [Collections.Generic.List[hashtable]]::new()
$taskName = 'workspace-proof-' + [guid]::NewGuid().ToString('N') + '.solidcraft'
function Rpc([hashtable]$Headers, [string]$Method, [hashtable]$Parameters = @{}) {
    $script:taskId++
    $body = @{jsonrpc = '2.0'; id = $script:taskId; method = $Method; params = $Parameters} | ConvertTo-Json -Depth 30 -Compress
    $reply = Invoke-WebRequest "$BaseUrl/mcp" -Method Post -ContentType application/json -Headers $Headers -Body $body
    if ($Method -eq 'initialize') { $Headers['Mcp-Session-Id'] = [string]($reply.Headers['Mcp-Session-Id'] | Select-Object -First 1) }
    $result = $reply.Content | ConvertFrom-Json -Depth 40
    if ($result.error) { throw 'MCP protocol error during workspace proof' }
    $result.result
}
function Client {
    $headers = @{Authorization = "Bearer $Token"; Accept = 'application/json, text/event-stream'; 'MCP-Protocol-Version' = '2025-06-18'}
    $null = Rpc $headers initialize @{protocolVersion = '2025-06-18'; capabilities = @{}; clientInfo = @{name = 'public-workspace-proof'; version = '1'}}
    $taskSessions.Add($headers)
    $headers
}
function Tool([hashtable]$Headers, [string]$Name, [hashtable]$Arguments = @{}, [switch]$AllowError) {
    $result = Rpc $Headers 'tools/call' @{name = $Name; arguments = $Arguments}
    if ($result.isError -and -not $AllowError) { throw "Workspace tool failed: $Name" }
    $result
}
function Read-WorkspaceJson($Result) {
    if ($Result.structuredContent) { return $Result.structuredContent }
    ($Result.content | Where-Object type -EQ text | Select-Object -First 1).text | ConvertFrom-Json -Depth 40
}
New-Item -ItemType Directory -Force $OutputDirectory | Out-Null
try {
    $a = Client
    $b = Client
    $catalog = Rpc $a 'tools/list'
    foreach ($name in @('session_status', 'workspace_list_files')) { if ($name -notin $catalog.tools.name) { throw 'Workspace tools missing' } }
    $null = Tool $a execute @{command = 'solid.box'; params = @{length = 30; width = 30; height = 40}}
    if (-not (Read-WorkspaceJson (Tool $a session_status)).dirty) { throw 'New model dirty state missing' }
    $null = Tool $a save @{path = $taskName}
    $saved = Read-WorkspaceJson (Tool $a session_status)
    if ($saved.current_file -ne $taskName -or $saved.dirty -or -not $saved.loaded_revision) { throw 'Saved file/status/revision incorrect' }
    $null = Tool $b open @{path = $taskName}
    $null = Tool $a execute @{command = 'solid.box'; params = @{length = 1; width = 2; height = 3; corner = @(50, 0, 0)}}
    $null = Tool $a save @{}
    $conflict = Tool $b save @{} -AllowError
    if (-not $conflict.isError -or ($conflict.content.text -join '') -notmatch 'revision conflict') { throw 'Stale model overwrote a newer file' }
    $measured = Read-WorkspaceJson (Tool $b measure)
    if ([Math]::Abs($measured.total.volume_mm3 - 36000) -gt 0.01) { throw 'Conflict lost the session model' }
    # Exporting another file must not bless the newer on-disk design as this session's base.
    $null = Tool $b export @{path = "$taskName.step"}
    if (-not (Tool $b save @{} -AllowError).isError) { throw 'Export bypassed stale-design protection' }
    $file = (Read-WorkspaceJson (Tool $a workspace_list_files @{path = $taskName})).files[0]
    $missingMatch = Invoke-WebRequest "$BaseUrl/files/$taskName" -Method Put -Headers @{Authorization = "Bearer $Token"} -Body 'blocked' -SkipHttpErrorCheck
    $staleMatch = Invoke-WebRequest "$BaseUrl/files/$taskName" -Method Put -Headers @{Authorization = "Bearer $Token"; 'If-Match' = '"stale"'} -Body 'blocked' -SkipHttpErrorCheck
    if ($missingMatch.StatusCode -ne 428 -or $staleMatch.StatusCode -ne 412) { throw 'HTTP PUT bypassed file revisions' }
    $bytes = (Invoke-WebRequest "$BaseUrl/files/$taskName" -Headers @{Authorization = "Bearer $Token"}).RawContentStream.ToArray()
    $put = Invoke-WebRequest "$BaseUrl/files/$taskName" -Method Put -Headers @{Authorization = "Bearer $Token"; 'If-Match' = $file.revision} -Body $bytes
    if ($put.StatusCode -ne 201 -or -not $put.Headers.ETag) { throw 'Matching HTTP revision was rejected' }
    $internal = @{jsonrpc = '2.0'; id = 9999; method = 'solidcraft/workspaceStatus'; params = @{}} | ConvertTo-Json -Compress
    if ((Invoke-WebRequest "$BaseUrl/mcp" -Method Post -ContentType application/json -Headers $a -Body $internal -SkipHttpErrorCheck).StatusCode -ne 400) { throw 'Internal telemetry exposed over HTTP' }
    if ((Invoke-WebRequest "$BaseUrl/workspace/api" -SkipHttpErrorCheck).StatusCode -ne 401) { throw 'Workspace leaked without login' }
    $login = Invoke-WebRequest "$BaseUrl/workspace/login" -SessionVariable browser
    if ($login.Headers.'Referrer-Policy' -ne 'same-origin') { throw 'Native form submissions would lose their Origin header' }
    if ($login.Content -notmatch 'name="csrf" value="([^"]+)"') { throw 'Login CSRF challenge missing' }
    $csrf = $Matches[1]
    $page = Invoke-WebRequest "$BaseUrl/workspace/login" -Method Post -WebSession $browser -Headers @{Origin = $BaseUrl} -Body @{token = $Token; csrf = $csrf}
    if ($page.Content -notmatch 'Dateien &amp; Sitzungen' -or $page.Content.Contains($Token)) { throw 'Authenticated dashboard missing or token reflected' }
    $snapshot = Invoke-RestMethod "$BaseUrl/workspace/api" -WebSession $browser
    $opened = @($snapshot.files | Where-Object name -EQ $taskName)
    if ($opened.Count -ne 1 -or $opened[0].opened_by.Count -ne 2 -or $snapshot.sessions.Count -ne 2) { throw 'Dashboard file/session association incorrect' }
    $snapshotText = $snapshot | ConvertTo-Json -Depth 30
    foreach ($secret in @($Token, $a['Mcp-Session-Id'], $b['Mcp-Session-Id'], '/data/files')) {
        if ($snapshotText.Contains($secret)) { throw 'Workspace API exposed private transport metadata' }
    }
    $filtered = Invoke-WebRequest "$BaseUrl/workspace?q=does-not-exist&kind=model" -WebSession $browser
    if ($filtered.Content -notmatch 'Keine passenden Dateien') { throw 'File filter did not work' }
    $download = Invoke-WebRequest "$BaseUrl/workspace/files/$taskName" -WebSession $browser
    if ($download.StatusCode -ne 200 -or $download.Headers.'Content-Disposition' -ne 'attachment' -or $download.Headers.'Cache-Control' -ne 'no-store') { throw 'Protected browser download failed' }
    ($page.Content -replace 'name="csrf" value="[^"]+"', 'name="csrf" value="[redacted]"') | Set-Content (Join-Path $OutputDirectory 'dashboard.html') -Encoding utf8
    # Evidence includes only public filenames, display aliases and model metadata.
    $snapshotText | Set-Content (Join-Path $OutputDirectory 'snapshot.json') -Encoding utf8
    if ($page.Content -notmatch 'name="csrf" value="([^"]+)"') { throw 'Logout CSRF missing' }
    $null = Invoke-WebRequest "$BaseUrl/workspace/logout" -Method Post -WebSession $browser -Headers @{Origin = $BaseUrl} -Body @{csrf = $Matches[1]}
    if ((Invoke-WebRequest "$BaseUrl/workspace/api" -WebSession $browser -SkipHttpErrorCheck).StatusCode -ne 401) { throw 'Logout did not revoke browser access' }
} finally {
    foreach ($session in $taskSessions) { try { $null = Invoke-WebRequest "$BaseUrl/mcp" -Method Delete -Headers $session } catch {} }
}
'PASS: workspace browser login/download/filter/logout, MCP metadata, real file associations, stale-write conflict/model preservation, conditional HTTP PUT and credential separation.'
