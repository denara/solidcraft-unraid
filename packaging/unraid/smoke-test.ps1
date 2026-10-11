param(
    [string]$BaseUrl = 'http://localhost:8781',
    [Parameter(Mandatory = $true)][string]$Token,
    [string]$OutputDirectory = 'target/unraid-proof',
    [switch]$Reopen
)
$ErrorActionPreference = 'Stop'
$BaseUrl = $BaseUrl.TrimEnd('/')
$taskHeaders = @{
    Authorization = "Bearer $Token"
    Accept = 'application/json, text/event-stream'
    'MCP-Protocol-Version' = '2025-06-18'
}
$taskRequestId = 0
function Invoke-Mcp([string]$Method, [hashtable]$Parameters = @{}) {
    $script:taskRequestId++
    $taskBody = @{jsonrpc = '2.0'; id = $script:taskRequestId; method = $Method; params = $Parameters} | ConvertTo-Json -Depth 50 -Compress
    $taskReply = Invoke-WebRequest "$BaseUrl/mcp" -Method Post -ContentType 'application/json' -Headers $taskHeaders -Body $taskBody -TimeoutSec 330
    if ($Method -eq 'initialize') { $taskHeaders['Mcp-Session-Id'] = [string]($taskReply.Headers['Mcp-Session-Id'] | Select-Object -First 1) }
    $taskJson = $taskReply.Content | ConvertFrom-Json -Depth 50
    if ($taskJson.error) { throw ($taskJson.error | ConvertTo-Json -Compress) }
    if ($taskJson.result.isError) { throw ($taskJson.result.content | ConvertTo-Json -Compress) }
    return $taskJson.result
}
function Invoke-Tool([string]$Name, [hashtable]$Arguments = @{}) {
    Invoke-Mcp 'tools/call' @{name = $Name; arguments = $Arguments}
}
function Initialize-Session {
    $null = Invoke-Mcp 'initialize' @{protocolVersion = '2025-06-18'; capabilities = @{}; clientInfo = @{name = 'solvecraft-unraid-proof'; version = '1'}}
    $taskBody = @{jsonrpc = '2.0'; method = 'notifications/initialized'} | ConvertTo-Json -Compress
    $taskReply = Invoke-WebRequest "$BaseUrl/mcp" -Method Post -ContentType 'application/json' -Headers $taskHeaders -Body $taskBody
    if ($taskReply.StatusCode -ne 202) { throw 'Initialized notification was not accepted' }
}
function Read-ToolJson($Result) {
    ($Result.content | Where-Object type -EQ 'text' | Select-Object -First 1).text | ConvertFrom-Json -Depth 50
}
New-Item -ItemType Directory -Force $OutputDirectory | Out-Null
Initialize-Session
if ($Reopen) {
    $null = Invoke-Tool 'open' @{path = 'unraid-proof.solvecraft'}
} else {
    $taskCatalog = Invoke-Mcp 'tools/list'
    if ($taskCatalog.tools.Count -lt 10) { throw 'MCP tool catalog is missing' }
    $null = Invoke-Tool 'batch' @{commands = @(
        @{command = 'sketch.create'; params = @{plane = 'XY'}},
        @{command = 'sketch.rectangle.two_point'; params = @{p0 = @(0, 0); p1 = @(40, 30)}},
        @{command = 'sketch.finish'; params = @{}},
        @{command = 'solid.extrude'; params = @{distance = 20}}
    )}
    $null = Invoke-Tool 'save' @{path = 'unraid-proof.solvecraft'}
    $null = Invoke-Tool 'export' @{path = 'unraid-proof.step'}
    Invoke-WebRequest "$BaseUrl/files/unraid-proof.step" -Headers @{Authorization = "Bearer $Token"} -OutFile (Join-Path $OutputDirectory 'unraid-proof.step')
    $taskStep = Get-Content (Join-Path $OutputDirectory 'unraid-proof.step') -TotalCount 1
    if ($taskStep -notmatch 'ISO-10303-21') { throw 'STEP download is invalid' }
}
$taskMeasured = Read-ToolJson (Invoke-Tool 'measure')
if ([Math]::Abs($taskMeasured.total.volume_mm3 - 24000) -gt 0.01) { throw "Unexpected volume: $($taskMeasured.total.volume_mm3)" }
$taskShot = Invoke-Tool 'screenshot' @{view = 'iso'; width = 800; height = 600; path = 'unraid-proof.png'}
$taskImage = ($taskShot.content | Where-Object type -EQ 'image' | Select-Object -First 1).data
$taskBytes = [Convert]::FromBase64String($taskImage)
if ($taskBytes.Length -lt 100 -or $taskBytes[0] -ne 137 -or $taskBytes[1] -ne 80) { throw 'PNG preview is invalid' }
[IO.File]::WriteAllBytes((Join-Path (Resolve-Path $OutputDirectory).Path 'unraid-proof.png'), $taskBytes)
if (-not $Reopen) {
    $taskFirstSession = $taskHeaders['Mcp-Session-Id']
    $taskHeaders.Remove('Mcp-Session-Id')
    Initialize-Session
    $taskEmpty = Read-ToolJson (Invoke-Tool 'measure')
    if ($taskEmpty.total.volume_mm3 -ne 0) { throw 'MCP sessions share an in-memory model' }
    $null = Invoke-WebRequest "$BaseUrl/mcp" -Method Delete -Headers $taskHeaders
    $taskHeaders['Mcp-Session-Id'] = $taskFirstSession
    $taskBadPath = @{jsonrpc = '2.0'; id = 999; method = 'tools/call'; params = @{name = 'save'; arguments = @{path = '../escape.solvecraft'}}} | ConvertTo-Json -Depth 10 -Compress
    $taskRejected = Invoke-RestMethod "$BaseUrl/mcp" -Method Post -ContentType 'application/json' -Headers $taskHeaders -Body $taskBadPath
    if (-not $taskRejected.result.isError) { throw 'Path traversal was not rejected' }
    $taskUnauthorized = Invoke-WebRequest "$BaseUrl/mcp" -Method Post -ContentType 'application/json' -Body '{}' -SkipHttpErrorCheck
    if ($taskUnauthorized.StatusCode -ne 401) { throw 'Unauthenticated MCP request was accepted' }
    $taskOriginHeaders = $taskHeaders.Clone()
    $taskOriginHeaders['Origin'] = 'https://attacker.invalid'
    $taskForbidden = Invoke-WebRequest "$BaseUrl/mcp" -Method Post -ContentType 'application/json' -Headers $taskOriginHeaders -Body '{}' -SkipHttpErrorCheck
    if ($taskForbidden.StatusCode -ne 403) { throw 'Untrusted origin was accepted' }
}
$null = Invoke-WebRequest "$BaseUrl/mcp" -Method Delete -Headers $taskHeaders
Write-Output "PASS: HTTP MCP modelling, 24000 mm3, PNG preview, separate sessions, persisted design$(if ($Reopen) { ' reopened after restart' } else { ', STEP download, authentication and path checks' })."
