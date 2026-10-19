param(
    [string]$BaseUrl = 'http://localhost:8781',
    [Parameter(Mandatory = $true)][string]$Token,
    [string]$OutputDirectory = 'target/unraid-project-proof'
)
# Disposable candidates only: creates public examples in unique project directories.
$ErrorActionPreference = 'Stop'
$BaseUrl = $BaseUrl.TrimEnd('/')
$taskId = 0
$taskSuffix = [guid]::NewGuid().ToString('N')
$taskProject = "project-proof-$taskSuffix"
$taskOther = "project-other-$taskSuffix"
$taskLegacy = "legacy-proof-$taskSuffix.solidcraft"
$taskSessions = [Collections.Generic.List[hashtable]]::new()
function Rpc([hashtable]$Headers, [string]$Method, [hashtable]$Parameters = @{}) {
    $script:taskId++
    $body = @{jsonrpc = '2.0'; id = $script:taskId; method = $Method; params = $Parameters} | ConvertTo-Json -Depth 40 -Compress
    $reply = Invoke-WebRequest "$BaseUrl/mcp" -Method Post -ContentType application/json -Headers $Headers -Body $body
    if ($Method -eq 'initialize') { $Headers['Mcp-Session-Id'] = [string]($reply.Headers['Mcp-Session-Id'] | Select-Object -First 1) }
    $result = $reply.Content | ConvertFrom-Json -Depth 50
    if ($result.error) { throw 'MCP protocol error during project proof' }
    $result.result
}
function Client {
    $headers = @{Authorization = "Bearer $Token"; Accept = 'application/json, text/event-stream'; 'MCP-Protocol-Version' = '2025-06-18'}
    $null = Rpc $headers initialize @{protocolVersion = '2025-06-18'; capabilities = @{}; clientInfo = @{name = 'public-project-proof'; version = '1'}}
    $taskSessions.Add($headers)
    $headers
}
function Tool([hashtable]$Headers, [string]$Name, [hashtable]$Arguments = @{}, [switch]$AllowError) {
    $result = Rpc $Headers 'tools/call' @{name = $Name; arguments = $Arguments}
    if ($result.isError -and -not $AllowError) { throw "Project proof tool failed: $Name; $($result.content.text -join ' ')" }
    $result
}
function ToolJson($Result) {
    if ($Result.structuredContent) { return $Result.structuredContent }
    ($Result.content | Where-Object type -EQ text | Select-Object -First 1).text | ConvertFrom-Json -Depth 50
}
function Reject([hashtable]$Headers, [string]$Name, [hashtable]$Arguments = @{}) {
    if (-not (Tool $Headers $Name $Arguments -AllowError).isError) { throw "Project boundary was bypassed by $Name" }
}
New-Item -ItemType Directory -Force $OutputDirectory | Out-Null
try {
    $a = Client
    $catalog = Rpc $a 'tools/list'
    foreach ($name in @('project_list', 'project_select')) {
        if ($name -notin $catalog.tools.name) { throw 'Project selection is not discoverable' }
    }
    $null = Tool $a project_list
    $null = Tool $a session_status
    Reject $a new_design
    Reject $a execute @{command = 'solid.box'; params = @{length = 1; width = 1; height = 1}}
    Reject $a save @{path = 'unselected.solidcraft'}
    foreach ($name in @('', '../escape', 'a/b', 'a\b', '.hidden', '..', 'trailing ', 'CON', 'NUL.txt')) {
        Reject $a project_select @{name = $name}
    }
    $null = Tool $a project_select @{name = $taskProject}
    $null = Tool $a project_select @{name = $taskProject}
    Reject $a project_select @{name = $taskOther}
    $status = ToolJson (Tool $a session_status)
    if ($status.project -ne $taskProject) { throw 'Session project metadata missing' }
    $null = Tool $a execute @{command = 'solid.box'; params = @{length = 10; width = 20; height = 30}}
    $null = Tool $a save @{path = 'example.solidcraft'; format = 'json'}
    $status = ToolJson (Tool $a session_status)
    if ($status.current_file -ne "$taskProject/example.solidcraft") { throw 'Saved file escaped the project or reported the wrong identity' }
    $null = Tool $a screenshot @{path = 'example-southeast.png'; view = 'iso'; width = 320; height = 240}
    $png = (ToolJson (Tool $a workspace_list_files @{path = "$taskProject/example-southeast.png"})).files[0]
    Reject $a screenshot @{path = 'example-southeast.png'; width = 320; height = 240; expected_revisions = @{'example-southeast.png' = $png.revision}}
    $null = Tool $a screenshot @{path = 'example-southeast.png'; width = 320; height = 240; expected_revisions = @{"$taskProject/example-southeast.png" = $png.revision}}
    $source = Invoke-WebRequest "$BaseUrl/files/$taskProject/example.solidcraft" -Headers @{Authorization = "Bearer $Token"}
    [byte[]]$bytes = $source.RawContentStream.ToArray()
    $null = Invoke-WebRequest "$BaseUrl/files/$taskLegacy" -Method Put -Headers @{Authorization = "Bearer $Token"} -Body $bytes
    foreach ($probeArguments in @(
        @{path = '../escape.solidcraft'},
        @{path = "/data/files/$taskOther/escape.solidcraft"}
    )) { Reject $a save $probeArguments }
    Reject $a batch @{commands = @(@{id = 'file.save'; params = @{path = '../escape.solidcraft'}})}
    Reject $a execute @{command = 'file.export'; params = @{path = "../$taskOther/escape.step"}}
    $null = Tool $a open @{path = $taskLegacy; from_workspace = $true}
    Reject $a save
    Reject $a execute @{command = 'file.save'; params = @{}}
    Reject $a batch @{commands = @(@{id = 'file.save'; params = @{}})}
    $null = Tool $a save @{path = 'imported.solidcraft'}
    $legacyAfter = Invoke-WebRequest "$BaseUrl/files/$taskLegacy" -Headers @{Authorization = "Bearer $Token"}
    if ([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)) -ne [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($legacyAfter.RawContentStream.ToArray()))) {
        throw 'Reading a legacy source modified its original file'
    }
    $b = Client
    $null = Tool $b project_select @{name = $taskOther}
    $null = Tool $b execute @{command = 'solid.box'; params = @{length = 1; width = 2; height = 3}}
    $null = Tool $b save @{path = 'example.solidcraft'}
    $measured = ToolJson (Tool $b measure)
    if ([Math]::Abs($measured.total.volume_mm3 - 6) -gt 0.001) { throw 'Projects shared their in-memory model' }
    $listing = ToolJson (Tool $a workspace_list_files @{path = "$taskProject/example.solidcraft"})
    if (@($listing.files).Count -ne 1 -or -not $listing.files[0].revision) { throw 'Project file revision lookup failed' }
    $null = Tool $a project_list
    @{project_selection = 'required'; binding = 'immutable'; files = 'project-relative'; revisions = 'workspace-relative'; legacy = 'read-source/save-copy'; traversal = 'rejected'; separate_projects = 'pass'} |
        ConvertTo-Json | Set-Content (Join-Path $OutputDirectory 'project-checks.json') -Encoding utf8
} finally {
    foreach ($session in $taskSessions) { try { $null = Invoke-WebRequest "$BaseUrl/mcp" -Method Delete -Headers $session } catch {} }
}
'PASS: mandatory immutable MCP project selection, real directories, path boundaries, separate models/files, legacy source preservation and shared revision identities.'
