param(
    [string]$BaseUrl = 'http://localhost:8781',
    [string]$ContainerName = 'proof',
    [ValidateSet('always', 'never')][string]$ColorMode = 'always',
    [Parameter(Mandatory = $true)][string]$Token,
    [string]$OutputDirectory = 'target/unraid-log-proof'
)
# Run only against a disposable proof container, after its restart/persistence check.
$ErrorActionPreference = 'Stop'
$taskHeaders = @{Authorization = "Bearer $Token"; Accept = 'application/json, text/event-stream'; 'MCP-Protocol-Version' = '2025-06-18'}
$taskSessions = [Collections.Generic.List[string]]::new()
$taskMarker = 'private-log-probe-' + [guid]::NewGuid().ToString('N')
$taskId = 0
function Read-Log {
    $taskRows = docker logs $ContainerName 2>&1
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read proof container logs' }
    $taskRows | ForEach-Object { [string]$_ }
}
function Request-Mcp([string]$Method, [hashtable]$Headers, [hashtable]$Parameters = @{}) {
    $script:taskId++
    $taskBody = @{jsonrpc = '2.0'; id = $script:taskId; method = $Method; params = $Parameters} | ConvertTo-Json -Depth 20 -Compress
    Invoke-WebRequest "$BaseUrl/mcp" -Method Post -Headers $Headers -ContentType application/json -Body $taskBody -SkipHttpErrorCheck
}
function Start-Session {
    $taskResponse = Request-Mcp initialize $taskHeaders @{protocolVersion = '2025-06-18'; capabilities = @{}; clientInfo = @{name = "$taskMarker-$Token"; version = '1'}}
    if ($taskResponse.StatusCode -ne 200) { throw 'Proof session initialization failed' }
    $taskSession = [string]($taskResponse.Headers['Mcp-Session-Id'] | Select-Object -First 1)
    if (-not $taskSession) { throw 'Missing proof session header' }
    $taskSessions.Add($taskSession)
    $taskSession
}
$taskBefore = @(Read-Log)
if (-not (($taskBefore -join "`n") -match 'server.ready' -and ($taskBefore -join "`n") -match 'server.stopping' -and ($taskBefore -join "`n") -match 'token.location.*SOLVECRAFT_MCP_TOKEN')) { throw 'Startup/shutdown/token-source events missing after proof restart' }
try {
    $taskFirst = Start-Session
    $taskActive = $taskHeaders.Clone()
    $taskActive['Mcp-Session-Id'] = $taskFirst
    $taskNotification = @{jsonrpc = '2.0'; method = 'notifications/initialized'} | ConvertTo-Json -Compress
    $null = Invoke-WebRequest "$BaseUrl/mcp" -Method Post -Headers $taskActive -ContentType application/json -Body $taskNotification
    for ($taskIndex = 0; $taskIndex -lt 6; $taskIndex++) {
        $taskReply = Request-Mcp ping $taskActive
        if ($taskReply.StatusCode -ne 200) { throw 'Routine MCP ping failed' }
        $null = Invoke-WebRequest "$BaseUrl/health"
        $null = Invoke-WebRequest "$BaseUrl/"
    }
    $taskQuiet = @(Read-Log)
    if ($taskQuiet.Count - $taskBefore.Count -ne 2) { throw 'Routine MCP/browser/health requests generated log noise' }
    $taskMissing = Invoke-WebRequest "$BaseUrl/mcp" -Method Post -ContentType application/json -Body '{}' -SkipHttpErrorCheck
    $taskInvalid = Invoke-WebRequest "$BaseUrl/mcp" -Method Post -Headers @{Authorization = "Bearer $taskMarker"} -ContentType application/json -Body '{}' -SkipHttpErrorCheck
    $taskBadOrigin = $taskActive.Clone()
    $taskBadOrigin.Origin = "https://$taskMarker.invalid"
    $taskForbidden = Invoke-WebRequest "$BaseUrl/mcp" -Method Post -Headers $taskBadOrigin -ContentType application/json -Body '{}' -SkipHttpErrorCheck
    if ($taskMissing.StatusCode -ne 401 -or $taskInvalid.StatusCode -ne 401 -or $taskForbidden.StatusCode -ne 403) { throw 'Authentication/origin rejection changed' }
    for ($taskIndex = 0; $taskIndex -lt 3; $taskIndex++) { $null = Start-Session }
    $taskFull = Request-Mcp initialize $taskHeaders @{protocolVersion = '2025-06-18'; capabilities = @{}; clientInfo = @{name = $taskMarker; version = '1'}}
    if ($taskFull.StatusCode -ne 503) { throw 'Proof container must have exactly four session slots' }
} finally {
    foreach ($taskSession in $taskSessions) {
        $taskClose = $taskHeaders.Clone()
        $taskClose['Mcp-Session-Id'] = $taskSession
        try { $null = Invoke-WebRequest "$BaseUrl/mcp" -Method Delete -Headers $taskClose } catch {}
    }
}
$taskAfter = @(Read-Log)
$taskAllText = $taskAfter -join "`n"
foreach ($taskPrivate in @($Token, $taskMarker) + @($taskSessions)) {
    if ($taskAllText.Contains($taskPrivate)) { throw 'Sensitive request data appeared in container logs' }
}
$taskRawDelta = @($taskAfter | Select-Object -Skip $taskBefore.Count)
$taskEscape = [string][char]27
$taskColors = @{ system = '36'; login = '95'; warn = '33'; error = '31' }
foreach ($taskRow in $taskRawDelta) {
    if ($ColorMode -eq 'always') {
        if ($taskRow -notmatch "^$taskEscape\[(36|95|33|31)m.+$taskEscape\[0m$") { throw 'Missing category color or final reset' }
        $taskCode = $Matches[1]
        if ($taskRow -notmatch '\[(system|login|warn|error)\] ' -or $taskColors[$Matches[1]] -ne $taskCode) { throw 'Wrong color for log category' }
    }
}
$taskDelta = @($taskRawDelta | ForEach-Object { $_ -replace "$taskEscape\[(36|95|33|31|0)m", '' })
if (($taskDelta -join "`n").Contains([char]27) -or ($ColorMode -eq 'never' -and $taskAllText.Contains([char]27))) { throw 'Unexpected ANSI escape sequences' }
foreach ($taskEvent in @('auth.accepted', 'auth.rejected', 'origin.rejected', 'session.opened', 'session.capacity_reached', 'session.released')) {
    if (-not (($taskDelta -join "`n").Contains($taskEvent))) { throw "Missing operational event: $taskEvent" }
}
$taskReleased = @($taskDelta | Where-Object { $_ -match 'session.released' })
if ($taskReleased.Count -ne 4 -or -not $taskReleased[-1].Contains('slots=0/4')) { throw 'Session capacity was not fully released in logs' }
foreach ($taskRow in $taskDelta) {
    if ($taskRow -notmatch '^\d{4}-\d{2}-\d{2}T\S+Z \[(system|login|warn|error)\] ') { throw 'Unexpected timestamp/severity format' }
    $taskStart = $taskRow.IndexOf('{')
    if ($taskStart -lt 0) { throw 'Missing structured operational fields' }
    $taskFields = $taskRow.Substring($taskStart) | ConvertFrom-Json
    if ($null -ne $taskFields.slots_used -and ($taskFields.slots_used -lt 0 -or $taskFields.slots_used -gt 4)) { throw 'Invalid slot count' }
}
New-Item -ItemType Directory -Force $OutputDirectory | Out-Null
$taskDelta | Set-Content (Join-Path $OutputDirectory 'operational.log') -Encoding utf8
Write-Output "PASS: authentication/session/capacity logs, UTC/severity/icons, color=$ColorMode, no credential or session-header leakage, no routine-request noise."
