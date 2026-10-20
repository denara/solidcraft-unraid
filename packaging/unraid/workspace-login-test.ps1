param(
    [string]$BaseUrl = 'http://localhost:8781',
    [Parameter(Mandatory = $true)][string]$Token,
    [Parameter(Mandatory = $true)][string]$ContainerName,
    [string]$CandidateTask = '',
    [string]$CandidateImage = ''
)
# This restart proof is restricted to labelled, loopback-only disposable containers.
# Cookies and credentials remain in memory; no proof file contains authentication data.
$ErrorActionPreference = 'Stop'
$BaseUrl = $BaseUrl.TrimEnd('/')
[Uri]$taskUri = $null
if (-not [Uri]::TryCreate($BaseUrl, [UriKind]::Absolute, [ref]$taskUri) -or $taskUri.Scheme -ne 'http' -or $taskUri.Host -notin @('localhost', '127.0.0.1', '[::1]', '::1') -or $taskUri.AbsolutePath -ne '/' -or $taskUri.Query -or $taskUri.Fragment -or $taskUri.UserInfo) {
    throw 'Restart proof requires an HTTP loopback origin without a path or credentials'
}
if ($ContainerName -notmatch '^[a-zA-Z0-9][a-zA-Z0-9_.-]{0,127}$' -or $Token.Length -lt 32) {
    throw 'An explicit disposable container name and proof credential are required'
}
$taskContainerId = $null

function Assert-DisposableCandidate {
    $identity = if ($taskContainerId) { $taskContainerId } else { $ContainerName }
    $inspectionText = docker inspect --type container $identity 2>$null
    if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect the requested disposable proof container' }
    try { $inspection = @($inspectionText -join "`n" | ConvertFrom-Json -Depth 50)[0] } catch { throw 'Invalid Docker inspection response' }
    $taskLabel = $inspection.Config.Labels.'solidcraft.task' -eq 'workspace-live-20261018'
    $proofLabel = $inspection.Config.Labels.'solidcraft.proof' -eq 'true'
    # Local builds must identify their exact task and image as well as all the
    # existing disposable-volume, loopback, isolation and credential safeguards.
    $explicitLocal = $proofLabel -and $CandidateTask -and $CandidateImage -and
        $inspection.Config.Labels.'solidcraft.task' -ceq $CandidateTask -and
        $inspection.Config.Image -ceq $CandidateImage -and
        $inspection.Name -ceq "/$ContainerName" -and
        $ContainerName -match '^(solidcraft|solvecraft)[a-z0-9._-]*(proof|test|candidate)[a-z0-9._-]*$'
    $taskLabel = $taskLabel -or $explicitLocal

    if (-not ($taskLabel -or $proofLabel) -or -not $inspection.State.Running -or $inspection.Id -notmatch '^[a-f0-9]{64}$') {
        throw 'Restart proof requires a running container explicitly labelled as a disposable proof'
    }
    if ($proofLabel -and -not $taskLabel -and ($inspection.Name -ne '/proof' -or $inspection.Config.Image -ne 'solvecraft-unraid:prototype')) {
        throw 'CI proof label is restricted to the canonical disposable proof container and image'
    }
    if ($taskContainerId -and $inspection.Id -ne $taskContainerId) { throw 'Proof container identity changed' }
    if ($inspection.HostConfig.Privileged -or -not $inspection.HostConfig.ReadonlyRootfs) { throw 'Proof container isolation settings do not match the disposable test workflow' }
    $bindings = @($inspection.NetworkSettings.Ports.'8080/tcp')
    if ($bindings.Count -ne 1 -or $bindings[0].HostIp -notin @('127.0.0.1', '::1') -or [string]$bindings[0].HostPort -ne [string]$taskUri.Port) {
        throw 'Proof URL does not match the container loopback port binding'
    }
    foreach ($port in $inspection.NetworkSettings.Ports.PSObject.Properties) {
        foreach ($binding in @($port.Value)) {
            if ($binding -and $binding.HostIp -notin @('127.0.0.1', '::1')) { throw 'Proof container publishes a non-loopback port' }
        }
    }
    $data = @($inspection.Mounts | Where-Object Destination -EQ '/data')
    if ($data.Count -ne 1 -or $data[0].Type -ne 'volume' -or -not $data[0].RW -or $data[0].Driver -ne 'local') {
        throw 'Proof container requires one writable local Docker test volume at /data'
    }
    $volume = [string]$data[0].Name
    if ($volume -ne 'proof-data' -and $volume -notmatch '^(solidcraft|solvecraft)[a-z0-9._-]*(proof|test|candidate)[a-z0-9._-]*$') {
        throw 'Data volume name does not identify a disposable proof, test or candidate volume'
    }
    foreach ($mount in $inspection.Mounts) {
        if ($mount.Type -eq 'bind' -or ($mount.Type -eq 'volume' -and $mount.Destination -ne '/data')) {
            throw 'Restart proof refuses bind mounts and additional data volumes'
        }
    }
    $credentials = @($inspection.Config.Env | Where-Object { $_.StartsWith('SOLVECRAFT_MCP_TOKEN=') })
    if ($credentials.Count -ne 1 -or $credentials[0].Substring('SOLVECRAFT_MCP_TOKEN='.Length) -cne $Token) {
        throw 'Requested proof credential does not match the disposable container configuration'
    }
    $script:taskContainerId = $inspection.Id
}

function Restart-DisposableCandidate {
    Assert-DisposableCandidate
    $null = docker restart --time 10 $taskContainerId 2>$null
    if ($LASTEXITCODE -ne 0) { throw 'Disposable proof container restart failed' }
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
        try {
            $health = Invoke-WebRequest "$BaseUrl/health" -TimeoutSec 2
            if ($health.StatusCode -eq 200) { return }
        } catch {}
        Start-Sleep -Milliseconds 500
    }
    throw 'Disposable proof container did not become healthy after restart'
}

function Login-Browser([string]$Duration) {
    $login = Invoke-WebRequest "$BaseUrl/workspace/login" -SessionVariable taskBrowser
    if ($login.Content -notmatch 'name="csrf" value="([^"]+)"') { throw 'Login challenge missing during restart proof' }
    $page = Invoke-WebRequest "$BaseUrl/workspace/login" -Method Post -WebSession $taskBrowser -Headers @{Origin = $BaseUrl} -Body @{csrf = $Matches[1]; token = $Token; remember = $Duration}
    if ($page.Content -notmatch 'name="csrf" value="([^"]+)"' -or $page.Content.Contains($Token)) { throw 'Authenticated workspace missing during restart proof' }
    @{browser = $taskBrowser; csrf = $Matches[1]}
}

function Assert-BrowserStatus($Browser, [int]$Expected) {
    $reply = Invoke-WebRequest "$BaseUrl/workspace/api" -WebSession $Browser.browser -SkipHttpErrorCheck
    if ($reply.StatusCode -ne $Expected) { throw 'Remembered browser login did not have the expected restart or revocation state' }
}

Assert-DisposableCandidate
$taskBrowsers = @{}
try {
    foreach ($duration in @('session', '7d', '30d', 'forever')) {
        $taskBrowsers[$duration] = Login-Browser $duration
        Assert-BrowserStatus $taskBrowsers[$duration] 200
    }
    Restart-DisposableCandidate
    Assert-BrowserStatus $taskBrowsers.session 401
    foreach ($duration in @('7d', '30d', 'forever')) { Assert-BrowserStatus $taskBrowsers[$duration] 200 }
    $null = Invoke-WebRequest "$BaseUrl/workspace/logout" -Method Post -WebSession $taskBrowsers['7d'].browser -Headers @{Origin = $BaseUrl} -Body @{csrf = $taskBrowsers['7d'].csrf}
    Assert-BrowserStatus $taskBrowsers['7d'] 401
    Restart-DisposableCandidate
    Assert-BrowserStatus $taskBrowsers.session 401
    Assert-BrowserStatus $taskBrowsers['7d'] 401
    foreach ($duration in @('30d', 'forever')) { Assert-BrowserStatus $taskBrowsers[$duration] 200 }
} finally {
    foreach ($duration in @('30d', 'forever')) {
        if ($taskBrowsers.ContainsKey($duration)) {
            try {
                $null = Invoke-WebRequest "$BaseUrl/workspace/logout" -Method Post -WebSession $taskBrowsers[$duration].browser -Headers @{Origin = $BaseUrl} -Body @{csrf = $taskBrowsers[$duration].csrf}
            } catch {}
        }
    }
    $taskBrowsers.Clear()
}
'PASS: session login expires on restart; 7-day, 30-day and indefinite logins survive; logout revocation survives a second isolated candidate restart.'
