param(
 [string]$BaseUrl='http://localhost:8781',
 [Parameter(Mandatory=$true)][string]$Token,
 [string]$OutputDirectory='target/unraid-workspace-upload-proof'
)
# Disposable candidate only. Creates original analytic public data, never edits deployment designs.
$ErrorActionPreference='Stop'
$BaseUrl=$BaseUrl.TrimEnd('/')
$project='Upload-Proof-'+[guid]::NewGuid().ToString('N')
$headers=@{Authorization="Bearer $Token";Accept='application/json, text/event-stream';'MCP-Protocol-Version'='2025-06-18'}
$requestId=0
function Rpc([string]$Method,[hashtable]$Params=@{}) {
 $script:requestId++
 $r=Invoke-WebRequest "$BaseUrl/mcp" -Method Post -ContentType application/json -Headers $headers -Body (@{jsonrpc='2.0';id=$requestId;method=$Method;params=$Params}|ConvertTo-Json -Depth 50 -Compress)
 if($Method -eq 'initialize'){$headers['Mcp-Session-Id']=[string]($r.Headers['Mcp-Session-Id']|Select-Object -First 1)}
 $v=$r.Content|ConvertFrom-Json -Depth 60
 if($v.error -or $v.result.isError){throw "MCP failed: $Method $($Params.name): $($v.result.content.text -join ' ')"}
 $v.result
}
function Tool([string]$Name,[hashtable]$Arguments=@{}) { Rpc 'tools/call' @{name=$Name;arguments=$Arguments} }
function ToolJson($v) {if($v.structuredContent){return $v.structuredContent};($v.content|Where-Object type -EQ text|Select-Object -First 1).text|ConvertFrom-Json -Depth 60}
function Init { $null=Rpc initialize @{protocolVersion='2025-06-18';capabilities=@{};clientInfo=@{name='public-upload-proof';version='1'}} }
function End {if($headers['Mcp-Session-Id']){$null=Invoke-WebRequest "$BaseUrl/mcp" -Method Delete -Headers $headers;$headers.Remove('Mcp-Session-Id')}}
function Post([string]$Path,$Data) { Invoke-WebRequest "$BaseUrl$Path" -Method Post -WebSession $browser -Headers $uiHeaders -ContentType application/json -Body ($Data|ConvertTo-Json -Depth 50 -Compress) -SkipHttpErrorCheck }
function Prepare([array]$Files) {
 $r=Post '/workspace/uploads/prepare' @{project=$project;files=$Files}
 if($r.StatusCode -ne 200){throw "Upload preparation failed: $($r.Content)"}
 $r.Content|ConvertFrom-Json -Depth 30
}
function Put($Batch,[int]$Index,[byte[]]$Bytes,[bool]$Overwrite=$false) {
 $h=@{}+$uiHeaders
 $h['X-Upload-Batch']=$Batch.batch;$h['X-Upload-Index']="$Index"
 if($Batch.files[$Index].revision){$h['If-Match']=$Batch.files[$Index].revision;if($Overwrite){$h['X-Upload-Overwrite']='true'}}else{$h['If-None-Match']='*'}
 Invoke-WebRequest "$BaseUrl/workspace/upload" -Method Put -WebSession $browser -Headers $h -ContentType application/octet-stream -Body $Bytes -SkipHttpErrorCheck
}
New-Item -ItemType Directory -Force $OutputDirectory|Out-Null
try {
 $login=Invoke-WebRequest "$BaseUrl/workspace/login" -SessionVariable browser
 if($login.Content -notmatch 'name="csrf" value="([^"]+)"'){throw 'No login challenge'}
 $page=Invoke-WebRequest "$BaseUrl/workspace/login" -Method Post -WebSession $browser -Headers @{Origin=$BaseUrl} -Body @{csrf=$Matches[1];token=$Token;remember='session'}
 if($page.Content -notmatch 'name="csrf" value="([^"]+)"'){throw 'No authenticated form'}
 $uiHeaders=@{Origin=$BaseUrl;'X-Workspace-CSRF'=$Matches[1]}
 foreach($bad in @('../escape','.private','CON','nested/name')) {if((Post '/workspace/projects' @{name=$bad}).StatusCode -ne 400){throw 'Invalid project name allowed'}}
 if((Post '/workspace/projects' @{name=$project}).StatusCode -ne 200){throw 'UI project creation failed'}
 if((Post '/workspace/projects' @{name=$project}).StatusCode -ne 409){throw 'Project collision not rejected'}
 $snapshot=Invoke-RestMethod "$BaseUrl/workspace/api" -WebSession $browser
 if($project -notin $snapshot.directories){throw 'Empty project invisible'}
 if($null -eq $snapshot.storage.available_bytes -or $snapshot.storage.total_bytes -le 0){throw 'Data capacity unavailable'}
 if((Invoke-WebRequest "$BaseUrl/workspace/projects" -Method Post -Headers @{Authorization="Bearer $Token";Origin=$BaseUrl} -ContentType application/json -Body '{"name":"NotAllowed"}' -SkipHttpErrorCheck).StatusCode -ne 401){throw 'Bearer accepted as browser auth'}
 foreach($badOrigin in @('https://evil.example.invalid','null')) {
  if((Invoke-WebRequest "$BaseUrl/workspace/projects" -Method Post -WebSession $browser -Headers @{Origin=$badOrigin;'X-Workspace-CSRF'=$uiHeaders['X-Workspace-CSRF']} -ContentType application/json -Body '{"name":"NotAllowed"}' -SkipHttpErrorCheck).StatusCode -ne 403){throw 'Foreign origin accepted'}
 }
 if((Invoke-WebRequest "$BaseUrl/workspace/projects" -Method Post -WebSession $browser -Headers @{Origin=$BaseUrl;'X-Workspace-CSRF'='wrong'} -ContentType application/json -Body '{"name":"NotAllowed"}' -SkipHttpErrorCheck).StatusCode -ne 403){throw 'Wrong CSRF accepted'}
 Init
 $listed=ToolJson (Tool project_list)
 if($project -notin $listed.projects){throw 'MCP cannot discover empty UI project'}
 $null=Tool project_select @{name=$project}
 $null=Tool execute @{command='solid.box';params=@{length=10;width=20;height=30}}
 $null=Tool save @{path='source.solidcraft'}
 $native=Invoke-WebRequest "$BaseUrl/files/$project/source.solidcraft" -Headers @{Authorization="Bearer $Token"}
 [byte[]]$nativeBytes=$native.RawContentStream.ToArray()
 $nativeHash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($nativeBytes))
 if((Post '/workspace/uploads/prepare' @{project=$project;files=@(@{name='blocked.svg';size=3})}).StatusCode -ne 409){throw 'Active project accepted upload'}
 End
 [byte[]]$svg=[Text.Encoding]::UTF8.GetBytes('<svg xmlns="http://www.w3.org/2000/svg" width="20mm" height="10mm" viewBox="0 0 20 10"><path d="M0 0L20 0L20 10L0 10Z"/></svg>')
 $files=@(@{name='uploaded.sc';size=$nativeBytes.Length},@{name='Schablone-ü.svg';size=$svg.Length})
 $batch=Prepare $files
 if((Put $batch 0 $nativeBytes).StatusCode -ne 200 -or (Put $batch 1 $svg).StatusCode -ne 200){throw 'Multiple UI uploads failed'}
 $batch=Prepare $files
 if((Put $batch 0 $nativeBytes).StatusCode -ne 409){throw 'Silent overwrite accepted'}
 if((Put $batch 0 $nativeBytes $true).StatusCode -ne 200){throw 'Confirmed overwrite failed'}
 $cancel=Post '/workspace/uploads/cancel' @{batch=$batch.batch}
 if((Put $batch 1 $svg $true).StatusCode -ne 409){throw 'Cancelled batch still writes'}
 $invalid=Prepare @(@{name='bad.png';size=$svg.Length})
 if((Put $invalid 0 $svg).StatusCode -ne 422){throw 'Invalid PNG accepted'}
 foreach($files in @(@(@{name='run.html';size=10}),@(@{name='large.step';size=16777217}),@(@{name='../escape.svg';size=10}))) {
  if((Post '/workspace/uploads/prepare' @{project=$project;files=$files}).StatusCode -eq 200){throw 'Unsafe upload plan accepted'}
 }
 Init
 $null=Tool project_select @{name=$project}
 $inventory=ToolJson (Tool workspace_list_files @{path="$project/uploaded.sc"})
 if($inventory.files.Count -ne 1){throw 'MCP cannot discover uploaded file'}
 $null=Tool open @{path='uploaded.sc'}
 $measure=ToolJson (Tool measure)
 if([Math]::Abs([double]$measure.total.volume_mm3-6000) -gt 0.001){throw 'Unexpected analytic volume'}
 $null=Tool execute @{command='solid.box';params=@{length=5;width=5;height=5;corner=@(30,0,0)}}
 $null=Tool save @{path='continued.sc'}
 $after=Invoke-WebRequest "$BaseUrl/files/$project/uploaded.sc" -Headers @{Authorization="Bearer $Token"}
 if([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($after.RawContentStream.ToArray())) -ne $nativeHash){throw 'Original upload bytes changed'}
 End
 $page=Invoke-WebRequest "$BaseUrl/workspace" -WebSession $browser
 foreach($text in @('Für alle Dateien übernehmen','upload-dropzone','upload-files','Speicher frei:')) {if(-not $page.Content.Contains($text)){throw "Missing upload UI: $text"}}
 if($page.Content.Contains($Token) -or $page.Content.Contains('/data/files')){throw 'UI leaked private config'}
 @{project=$project;manual_project=$true;empty_project_mcp_discovery=$true;multiple_uploads=$true;owner_origin_csrf=$true;explicit_overwrite_cancel=$true;content_and_limits=$true;storage_volume_bytes=$snapshot.storage;MCP_open_analyse_continue_original_unchanged=$true}|ConvertTo-Json -Depth 12|Set-Content "$OutputDirectory/proof.json" -Encoding utf8
 'PASS: UI project/upload authentication, bounds, overwrite and real MCP continuation'
} finally {End}
