param(
    [string]$BaseUrl = 'http://localhost:8781',
    [Parameter(Mandatory = $true)][string]$Token,
    [string]$OutputDirectory = 'target/unraid-workspace-live-proof'
)
# Run only against a disposable candidate. Creates its own public model and export;
# never restarts a container, deletes server files, or changes existing designs.
$ErrorActionPreference = 'Stop'
$BaseUrl = $BaseUrl.TrimEnd('/')
$taskStem = 'workspace-live-proof-' + [guid]::NewGuid().ToString('N')
$taskModel = "$taskStem.solidcraft"
$taskExport = "$taskStem.step"
$taskHeaders = @{Authorization = "Bearer $Token"; Accept = 'application/json, text/event-stream'; 'MCP-Protocol-Version' = '2025-06-18'}
$taskRequestId = 0
$taskEventCount = 0
$taskReader = $null
$taskResponse = $null
$taskClient = $null

function Rpc([string]$Method, [hashtable]$Parameters = @{}) {
    $script:taskRequestId++
    $body = @{jsonrpc = '2.0'; id = $script:taskRequestId; method = $Method; params = $Parameters} | ConvertTo-Json -Depth 40 -Compress
    $reply = Invoke-WebRequest "$BaseUrl/mcp" -Method Post -ContentType application/json -Headers $taskHeaders -Body $body
    if ($Method -eq 'initialize') { $taskHeaders['Mcp-Session-Id'] = [string]($reply.Headers['Mcp-Session-Id'] | Select-Object -First 1) }
    $result = $reply.Content | ConvertFrom-Json -Depth 50
    if ($result.error -or $result.result.isError) { throw 'MCP request failed during live workspace proof' }
    $result.result
}
function Tool([string]$Name, [hashtable]$Arguments = @{}) { Rpc 'tools/call' @{name = $Name; arguments = $Arguments} }
function Tool-Json($Result) {
    if ($Result.structuredContent) { return $Result.structuredContent }
    ($Result.content | Where-Object type -EQ text | Select-Object -First 1).text | ConvertFrom-Json -Depth 50
}
function Read-SseEvent([int]$TimeoutSeconds = 15) {
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $eventName = 'message'
    $data = [Text.StringBuilder]::new()
    while ($timer.Elapsed.TotalSeconds -lt $TimeoutSeconds) {
        $remaining = [Math]::Max(1, [int](($TimeoutSeconds - $timer.Elapsed.TotalSeconds) * 1000))
        $lineTask = $taskReader.ReadLineAsync()
        if (-not $lineTask.Wait($remaining)) { throw 'Timed out waiting for a live workspace event' }
        $line = $lineTask.GetAwaiter().GetResult()
        if ($null -eq $line) { throw 'Live workspace event stream closed unexpectedly' }
        if ($line.Length -eq 0 -and $data.Length -gt 0) {
            $payload = $data.ToString() | ConvertFrom-Json -Depth 15
            if ($payload.html -and ($payload.html.Contains($Token) -or $payload.html.Contains('/data/files') -or $payload.html.Contains($taskHeaders['Mcp-Session-Id']))) {
                throw 'Live workspace event exposed private transport metadata'
            }
            if ($eventName -eq 'workspace') { $script:taskEventCount++ }
            return @{name = $eventName; data = $payload}
        }
        if ($line.StartsWith('event:')) { $eventName = $line.Substring(6).Trim() }
        if ($line.StartsWith('data:')) { $null = $data.AppendLine($line.Substring(5).TrimStart()) }
    }
    throw 'Live workspace event was not received'
}
function Wait-Workspace([scriptblock]$Predicate) {
    for ($attempt = 0; $attempt -lt 20; $attempt++) {
        $event = Read-SseEvent
        if ($event.name -eq 'workspace' -and (& $Predicate $event.data.html)) { return $event.data.html }
        if ($event.name -in @('expired', 'unavailable')) { throw 'Live workspace became unavailable during proof' }
    }
    throw 'Expected workspace update was not received'
}

New-Item -ItemType Directory -Force $OutputDirectory | Out-Null
try {
    $null = Rpc initialize @{protocolVersion = '2025-06-18'; capabilities = @{}; clientInfo = @{name = 'public-workspace-live-proof'; version = '1'}}
    $null = Tool execute @{command = 'solid.box'; params = @{length = 10; width = 20; height = 30}}
    $login = Invoke-WebRequest "$BaseUrl/workspace/login" -SessionVariable browser
    if ($login.Content -notmatch 'name="csrf" value="([^"]+)"') { throw 'Browser login challenge missing' }
    $page = Invoke-WebRequest "$BaseUrl/workspace/login" -Method Post -WebSession $browser -Headers @{Origin = $BaseUrl} -Body @{csrf = $Matches[1]; token = $Token; remember = 'session'}
    if ($page.Content -notmatch 'name="csrf" value="([^"]+)"') { throw 'Authenticated workspace missing' }
    $logoutCsrf = $Matches[1]
    $unauthorized = Invoke-WebRequest "$BaseUrl/workspace/events" -Headers @{Authorization = "Bearer $Token"} -SkipHttpErrorCheck
    if ($unauthorized.StatusCode -ne 204) { throw 'Bearer credentials were accepted as browser event authentication' }

    $handler = [Net.Http.HttpClientHandler]::new()
    $handler.CookieContainer = $browser.Cookies
    $handler.AllowAutoRedirect = $false
    $taskClient = [Net.Http.HttpClient]::new($handler)
    $taskClient.Timeout = [Threading.Timeout]::InfiniteTimeSpan
    $request = [Net.Http.HttpRequestMessage]::new([Net.Http.HttpMethod]::Get, "$BaseUrl/workspace/events")
    $request.Headers.Accept.ParseAdd('text/event-stream')
    $taskResponse = $taskClient.SendAsync($request, [Net.Http.HttpCompletionOption]::ResponseHeadersRead).GetAwaiter().GetResult()
    if ([int]$taskResponse.StatusCode -ne 200 -or $taskResponse.Content.Headers.ContentType.MediaType -ne 'text/event-stream') { throw 'Authenticated event stream unavailable' }
    $stream = $taskResponse.Content.ReadAsStreamAsync().GetAwaiter().GetResult()
    $taskReader = [IO.StreamReader]::new($stream)
    $null = Wait-Workspace { param($html) -not $html.Contains($taskModel) }

    $null = Tool execute @{command = 'file.save'; params = @{path = $taskModel; format = 'json'}}
    $created = Wait-Workspace { param($html) $html.Contains($taskModel) }
    $listed = Tool-Json (Tool workspace_list_files @{path = $taskModel})
    $revision = $listed.files[0].revision
    if (-not $revision) { throw 'Saved design revision missing' }
    $download = Invoke-WebRequest "$BaseUrl/files/$taskModel" -Headers @{Authorization = "Bearer $Token"}
    $document = [Text.Encoding]::UTF8.GetString($download.RawContentStream.ToArray()) | ConvertFrom-Json -AsHashtable -Depth 100
    $document.name = 'Live workspace proof updated'
    $updatedBytes = [Text.Encoding]::UTF8.GetBytes(($document | ConvertTo-Json -Depth 100 -Compress))
    $put = Invoke-WebRequest "$BaseUrl/files/$taskModel" -Method Put -Headers @{Authorization = "Bearer $Token"; 'If-Match' = $revision} -Body $updatedBytes
    if ($put.StatusCode -ne 201 -or -not $put.Headers.ETag) { throw 'Conditional model replacement failed' }
    $replacement = Wait-Workspace { param($html) $html.Contains($taskModel) -and $html -ne $created }
    $null = Tool export @{path = $taskExport}
    $grouped = Wait-Workspace { param($html) $html.Contains($taskModel) -and $html.Contains($taskExport) }

    $snapshot = Invoke-RestMethod "$BaseUrl/workspace/api" -WebSession $browser
    $model = @($snapshot.files | Where-Object name -EQ $taskModel)[0]
    $export = @($snapshot.files | Where-Object name -EQ $taskExport)[0]
    if (-not $model.project.id -or $model.project.id -ne $export.project.id -or $model.project.basis -ne 'design_stem') { throw 'Model/export project grouping incorrect' }
    $details = Invoke-WebRequest "$BaseUrl/workspace/details/$taskModel" -WebSession $browser
    if ($details.Content -notmatch 'Live workspace proof updated' -or $details.Content -notmatch '<dt>Features</dt><dd>1</dd>') { throw 'Saved model properties missing or stale' }
    if ($details.Content.Contains($Token) -or $details.Content.Contains('/data/files')) { throw 'Model details exposed private transport metadata' }
    if ((Invoke-WebRequest "$BaseUrl/workspace/details/$taskModel" -Headers @{Authorization = "Bearer $Token"} -SkipHttpErrorCheck).StatusCode -ne 401) { throw 'Details accepted Bearer authentication as a browser login' }

    $null = Invoke-WebRequest "$BaseUrl/workspace/logout" -Method Post -WebSession $browser -Headers @{Origin = $BaseUrl} -Body @{csrf = $logoutCsrf}
    $expired = $null
    for ($attempt = 0; $attempt -lt 10; $attempt++) {
        $expired = Read-SseEvent
        if ($expired.name -eq 'expired') { break }
    }
    if ($expired.name -ne 'expired') { throw 'Logout did not terminate the open event stream' }
    $grouped | Set-Content (Join-Path $OutputDirectory 'live-fragment.html') -Encoding utf8
    $details.Content | Set-Content (Join-Path $OutputDirectory 'saved-model-details.html') -Encoding utf8
    @{transport = 'SSE'; workspace_events = $taskEventCount; model = $taskModel; export = $taskExport; project = $model.project; file_creation = 'pass'; conditional_replacement = 'pass'; grouping = 'pass'; saved_properties = 'pass'; cookie_only_authentication = 'pass'; logout_closes_stream = 'pass'} |
        ConvertTo-Json -Depth 10 | Set-Content (Join-Path $OutputDirectory 'live-checks.json') -Encoding utf8
} finally {
    if ($taskReader) { $taskReader.Dispose() }
    if ($taskResponse) { $taskResponse.Dispose() }
    if ($taskClient) { $taskClient.Dispose() }
    if ($taskHeaders['Mcp-Session-Id']) { try { $null = Invoke-WebRequest "$BaseUrl/mcp" -Method Delete -Headers $taskHeaders } catch {} }
}
'PASS: cookie-only SSE, live saved-file creation and conditional replacement, model/export projects, saved properties and immediate logout revocation.'
