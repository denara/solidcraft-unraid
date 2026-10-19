param(
    [string]$BaseUrl = 'http://localhost:8781',
    [Parameter(Mandatory = $true)][string]$Token,
    [string]$OutputDirectory = 'target/unraid-observer-proof'
)
# Disposable candidates only. Modelling stays in RAM; observing must create no shared files.
$ErrorActionPreference = 'Stop'
$BaseUrl = $BaseUrl.TrimEnd('/')
$taskProject = 'observer-proof-' + [guid]::NewGuid().ToString('N')
$taskHeaders = @{Authorization = "Bearer $Token"; Accept = 'application/json, text/event-stream'; 'MCP-Protocol-Version' = '2025-06-18'}
$taskId = 0
$taskStreams = [Collections.Generic.List[hashtable]]::new()
function Rpc([string]$Method, [hashtable]$Parameters = @{}) {
    $script:taskId++
    $body = @{jsonrpc = '2.0'; id = $script:taskId; method = $Method; params = $Parameters} | ConvertTo-Json -Depth 40 -Compress
    $reply = Invoke-WebRequest "$BaseUrl/mcp" -Method Post -ContentType application/json -Headers $taskHeaders -Body $body
    if ($Method -eq 'initialize') { $taskHeaders['Mcp-Session-Id'] = [string]($reply.Headers['Mcp-Session-Id'] | Select-Object -First 1) }
    $result = $reply.Content | ConvertFrom-Json -Depth 50
    if ($result.error -or $result.result.isError) { throw "Observer proof MCP failed: $Method $($Parameters.name)" }
    $result.result
}
function Tool([string]$Name, [hashtable]$Arguments = @{}) { Rpc 'tools/call' @{name = $Name; arguments = $Arguments} }
function ToolJson($Result) {
    if ($Result.structuredContent) { return $Result.structuredContent }
    ($Result.content | Where-Object type -EQ text | Select-Object -First 1).text | ConvertFrom-Json -Depth 50
}
function Login {
    $challenge = Invoke-WebRequest "$BaseUrl/workspace/login" -SessionVariable browser
    if ($challenge.Content -notmatch 'name="csrf" value="([^"]+)"') { throw 'Browser challenge missing' }
    $page = Invoke-WebRequest "$BaseUrl/workspace/login" -WebSession $browser -Method Post -Headers @{Origin = $BaseUrl} -Body @{csrf = $Matches[1]; token = $Token}
    if ($page.Content -notmatch 'name="csrf" value="([^"]+)"') { throw 'Browser login failed' }
    @{browser = $browser; csrf = $Matches[1]}
}
function Open-Observer($Browser, [string]$Url) {
    $handler = [Net.Http.HttpClientHandler]::new()
    $handler.CookieContainer = $Browser.Cookies
    $handler.AllowAutoRedirect = $false
    $client = [Net.Http.HttpClient]::new($handler)
    $client.Timeout = [Threading.Timeout]::InfiniteTimeSpan
    $request = [Net.Http.HttpRequestMessage]::new([Net.Http.HttpMethod]::Get, $Url)
    $request.Headers.Accept.ParseAdd('text/event-stream')
    $response = $client.SendAsync($request, [Net.Http.HttpCompletionOption]::ResponseHeadersRead).GetAwaiter().GetResult()
    if ([int]$response.StatusCode -ne 200 -or $response.Content.Headers.ContentType.MediaType -ne 'text/event-stream') { $client.Dispose(); throw 'Observer SSE unavailable' }
    $entry = @{client = $client; response = $response; reader = [IO.StreamReader]::new($response.Content.ReadAsStreamAsync().GetAwaiter().GetResult())}
    $taskStreams.Add($entry)
    $entry
}
function Read-Observer($Stream, [int]$TimeoutSeconds = 30) {
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $eventName = 'message'
    $data = [Text.StringBuilder]::new()
    while ($timer.Elapsed.TotalSeconds -lt $TimeoutSeconds) {
        $read = $Stream.reader.ReadLineAsync()
        $remaining = [Math]::Max(1, [int](($TimeoutSeconds - $timer.Elapsed.TotalSeconds) * 1000))
        if (-not $read.Wait($remaining)) { throw 'Observer event timed out' }
        $line = $read.GetAwaiter().GetResult()
        if ($null -eq $line) { throw 'Observer stream ended without a terminal event' }
        if ($line -eq '' -and $data.Length) {
            $text = $data.ToString()
            foreach ($secret in @($Token, $taskHeaders['Mcp-Session-Id'], '/data/files')) {
                if ($secret -and $text.Contains($secret)) { throw 'Observer leaked transport credentials or private paths' }
            }
            return @{name = $eventName; data = ($text | ConvertFrom-Json -Depth 15)}
        }
        if ($line.StartsWith('event:')) { $eventName = $line.Substring(6).Trim() }
        if ($line.StartsWith('data:')) { $null = $data.AppendLine($line.Substring(5).TrimStart()) }
    }
    throw 'No observer event received'
}
function Wait-Frame($Stream, [long]$After = -1) {
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
        $event = Read-Observer $Stream
        if ($event.name -eq 'observer' -and $event.data.status -eq 'ready' -and $event.data.revision -gt $After) { return $event.data }
        if ($event.name -eq 'expired' -or $event.data.status -in @('ended', 'unavailable')) { throw 'Observer failed before the expected frame' }
    }
    throw 'Expected model revision was not rendered'
}
function Image-Bytes($Browser, $Frame, [string]$Name) {
    if ($Frame.image_url -notmatch '^/workspace/observe/[a-fA-F0-9-]+/image\?revision=\d+$') { throw 'Invalid observer image URL' }
    $reply = Invoke-WebRequest "$BaseUrl$($Frame.image_url)" -WebSession $Browser
    if ($reply.StatusCode -ne 200 -or $reply.Headers.'Content-Type' -ne 'image/png' -or $reply.Headers.'Cache-Control' -ne 'no-store' -or $reply.Headers.'X-Content-Type-Options' -ne 'nosniff') { throw 'Observer image security headers incorrect' }
    [byte[]]$bytes = $reply.RawContentStream.ToArray()
    if ($bytes.Length -lt 100 -or $bytes.Length -gt 2097152 -or [Convert]::ToHexString($bytes[0..7]) -ne '89504E470D0A1A0A') { throw 'Invalid bounded observer PNG' }
    $width = [Convert]::ToUInt32([Convert]::ToHexString($bytes[16..19]), 16)
    $height = [Convert]::ToUInt32([Convert]::ToHexString($bytes[20..23]), 16)
    if ($width -ne 800 -or $height -ne 500) { throw 'Unexpected observer resolution' }
    [IO.File]::WriteAllBytes((Join-Path (Resolve-Path $OutputDirectory).Path $Name), $bytes)
    ,$bytes
}
New-Item -ItemType Directory -Force $OutputDirectory | Out-Null
try {
    $null = Rpc initialize @{protocolVersion = '2025-06-18'; capabilities = @{}; clientInfo = @{name = 'public-observer-proof'; version = '1'}}
    $null = Tool project_select @{name = $taskProject}
    $beforeFiles = (ToolJson (Tool workspace_list_files)) | ConvertTo-Json -Depth 30 -Compress
    $loginA = Login
    $status = ToolJson (Tool session_status)
    $snapshot = Invoke-RestMethod "$BaseUrl/workspace/api" -WebSession $loginA.browser
    $session = @($snapshot.sessions | Where-Object display_id -EQ $status.display_id)[0]
    $handle = [string]$session.observer_handle
    if ($handle -notmatch '^[a-f0-9-]{36}$' -or $handle -eq $taskHeaders['Mcp-Session-Id']) { throw 'Observer has no distinct opaque session handle' }
    $eventsUrl = "$BaseUrl/workspace/observe/$handle/events"
    $imageUrl = "$BaseUrl/workspace/observe/$handle/image?revision=1"
    $bearerEvents = Invoke-WebRequest $eventsUrl -Headers @{Authorization = "Bearer $Token"} -SkipHttpErrorCheck
    if ($bearerEvents.StatusCode -notin @(204, 401)) { throw 'Observer events accepted Bearer instead of browser cookie' }
    if ((Invoke-WebRequest $imageUrl -Headers @{Authorization = "Bearer $Token"} -SkipHttpErrorCheck).StatusCode -ne 401) { throw 'Observer image accepted Bearer-only authentication' }
    if ((Invoke-WebRequest $eventsUrl -WebSession $loginA.browser -Headers @{Origin = 'https://attacker.invalid'} -SkipHttpErrorCheck).StatusCode -ne 403) { throw 'Cross-origin observer subscription accepted' }
    # Invoke-WebRequest retains explicit headers on WebRequestSession. Remove
    # this deliberate hostile Origin before the following ordinary image GETs.
    $null = $loginA.browser.Headers.Remove('Origin')
    foreach ($method in @('solidcraft/workspaceStatus', 'solidcraft/workspaceSnapshot')) {
        $body = @{jsonrpc = '2.0'; id = 999; method = $method; params = @{action = 'start'}} | ConvertTo-Json -Compress
        if ((Invoke-WebRequest "$BaseUrl/mcp" -Method Post -Headers $taskHeaders -ContentType application/json -Body $body -SkipHttpErrorCheck).StatusCode -ne 400) { throw 'Private worker observer protocol exposed through MCP HTTP' }
    }
    $null = Tool batch @{commands = @(
        @{command = 'sketch.create'; params = @{plane = 'XY'}},
        @{command = 'sketch.rectangle.two_point'; params = @{p0 = @(0, 0); p1 = @(30, 30)}}
    )}
    $a = Open-Observer $loginA.browser $eventsUrl
    $sketch = Wait-Frame $a
    [byte[]]$sketchBytes = Image-Bytes $loginA.browser $sketch 'unsaved-sketch.png'
    $null = Tool batch @{commands = @(
        @{command = 'sketch.finish'; params = @{}},
        @{command = 'solid.extrude'; params = @{distance = 40}}
    )}
    $bodyFrame = Wait-Frame $a $sketch.revision
    [byte[]]$bodyBytes = Image-Bytes $loginA.browser $bodyFrame 'unsaved-body.png'
    if ([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($sketchBytes)) -eq [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bodyBytes))) { throw 'Unsaved sketch and body have identical previews' }
    $beforeObserve = ToolJson (Tool session_status)
    $beforeInspect = ToolJson (Tool inspect_design)
    $loginB = Login
    $b = Open-Observer $loginB.browser $eventsUrl
    $sameFrame = Wait-Frame $b
    if ($sameFrame.revision -ne $bodyFrame.revision -or $sameFrame.image_url -ne $bodyFrame.image_url -or $sameFrame.captured_at -ne $bodyFrame.captured_at) { throw 'Viewers did not share the latest bounded frame' }
    for ($attempt = 0; $attempt -lt 12; $attempt++) { $null = Tool inspect_design }
    $afterObserve = ToolJson (Tool session_status)
    $afterInspect = ToolJson (Tool inspect_design)
    foreach ($property in @('current_file', 'dirty', 'model_revision')) {
        if ($beforeObserve.$property -ne $afterObserve.$property) { throw "Observation changed the model: $property" }
    }
    if (-not $afterObserve.dirty -or $afterObserve.current_file -or $beforeInspect.undo -ne $afterInspect.undo) { throw 'Observing changed the unsaved model or its undo history' }
    $null = Tool screenshot @{view = 'top'; width = 160; height = 100}
    $top = Wait-Frame $a $bodyFrame.revision
    [byte[]]$topBytes = Image-Bytes $loginA.browser $top 'unsaved-top.png'
    if ((ToolJson (Tool session_status)).model_revision -ne $afterObserve.model_revision) { throw 'Camera-only change altered document revision' }
    if ([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bodyBytes)) -eq [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($topBytes))) { throw 'Camera-only changes were not observed' }
    if ((Invoke-WebRequest "$BaseUrl$($bodyFrame.image_url)" -WebSession $loginA.browser -SkipHttpErrorCheck).StatusCode -notin @(404, 409)) { throw 'Old frame URL was silently replaced by a different revision' }
    $afterFiles = (ToolJson (Tool workspace_list_files)) | ConvertTo-Json -Depth 30 -Compress
    if ($beforeFiles -cne $afterFiles) { throw 'Observing created or modified shared files' }
    $measure = ToolJson (Tool measure)
    if ([Math]::Abs($measure.total.volume_mm3 - 36000) -gt 0.01) { throw 'Observed model has incorrect volume' }
    $null = Invoke-WebRequest "$BaseUrl/workspace/logout" -WebSession $loginA.browser -Method Post -Headers @{Origin = $BaseUrl} -Body @{csrf = $loginA.csrf}
    if ((Invoke-WebRequest "$BaseUrl$($top.image_url)" -WebSession $loginA.browser -SkipHttpErrorCheck).StatusCode -ne 401) { throw 'Logout did not revoke observer image access' }
    $expired = Read-Observer $a
    for ($attempt = 0; $attempt -lt 15 -and $expired.name -ne 'expired'; $attempt++) { $expired = Read-Observer $a }
    if ($expired.name -ne 'expired') { throw 'Logout did not close observation' }
    $null = Invoke-WebRequest "$BaseUrl/mcp" -Method Delete -Headers $taskHeaders
    $taskHeaders.Remove('Mcp-Session-Id')
    $ended = Read-Observer $b
    for ($attempt = 0; $attempt -lt 15 -and $ended.data.status -ne 'ended'; $attempt++) { $ended = Read-Observer $b }
    if ($ended.data.status -ne 'ended') { throw 'Ended CAD session left observer live' }
    if ((Invoke-WebRequest "$BaseUrl$($top.image_url)" -WebSession $loginB.browser -SkipHttpErrorCheck).StatusCode -ne 404) { throw 'Cached frame remained accessible after its session ended' }
    @{unsaved_sketch = 'pass'; unsaved_body_mm3 = 36000; camera_only_revision = 'pass'; shared_frame = 'pass'; model_history_preserved = 'pass'; mcp_responsive = 'pass'; shared_files_unchanged = 'pass'; cookie_only_auth = 'pass'; logout_revocation = 'pass'; session_end_revocation = 'pass'} |
        ConvertTo-Json | Set-Content (Join-Path $OutputDirectory 'observer-checks.json') -Encoding utf8
} finally {
    foreach ($stream in $taskStreams) { $stream.reader.Dispose(); $stream.response.Dispose(); $stream.client.Dispose() }
    if ($taskHeaders['Mcp-Session-Id']) { try { $null = Invoke-WebRequest "$BaseUrl/mcp" -Method Delete -Headers $taskHeaders } catch {} }
}
'PASS: real unsaved sketch/body/camera snapshots, shared bounded frames, preserved model/history/files, responsive MCP, cookie authentication, logout and ended-session revocation.'
