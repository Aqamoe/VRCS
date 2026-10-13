[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$sourceUrl = 'https://models.dev/api.json'
$response = Invoke-WebRequest -Uri $sourceUrl -TimeoutSec 30
$rawBytes = [Text.Encoding]::UTF8.GetBytes($response.Content)
if ($rawBytes.Length -gt 16MB) { throw 'The models.dev catalog exceeds the download limit' }
$catalog = $response.Content | ConvertFrom-Json
$providerIds = @('openai', 'google', 'groq', 'deepseek', 'openrouter', 'alibaba', 'alibaba-cn', 'alibaba-token-plan-cn')
$modelFields = @('modalities', 'limit', 'reasoning', 'reasoning_options', 'temperature', 'structured_output', 'tool_call', 'status', 'canonical_model_id')
$providers = [ordered]@{}
foreach ($providerId in $providerIds) {
    $provider = $catalog.PSObject.Properties[$providerId].Value
    if (-not $provider.models) { throw "Missing provider: $providerId" }
    $models = [ordered]@{}
    foreach ($model in $provider.models.PSObject.Properties) {
        $entry = [ordered]@{}
        foreach ($field in $modelFields) {
            $property = $model.Value.PSObject.Properties[$field]
            if ($null -ne $property) { $entry[$field] = $property.Value }
        }
        $models[$model.Name] = $entry
    }
    $providers[$providerId] = @{ models = $models }
}
$timestamp = [DateTimeOffset]::UtcNow
$snapshot = @{ updated_at = $timestamp.ToUnixTimeSeconds(); providers = $providers } | ConvertTo-Json -Depth 30 -Compress
$sha = [Security.Cryptography.SHA256]::Create()
try { $digest = ([BitConverter]::ToString($sha.ComputeHash($rawBytes))).Replace('-', '').ToLowerInvariant() }
finally { $sha.Dispose() }
$provenance = [ordered]@{ source = $sourceUrl; fetched_at = $timestamp.ToString('o'); etag = [string]$response.Headers['ETag']; sha256 = $digest } | ConvertTo-Json
$license = (Invoke-WebRequest -Uri 'https://raw.githubusercontent.com/anomalyco/models.dev/dev/LICENSE' -TimeoutSec 30).Content
$target = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\core\assets\models-dev'))
[IO.Directory]::CreateDirectory($target) | Out-Null
$encoding = [Text.UTF8Encoding]::new($false)
[IO.File]::WriteAllText((Join-Path $target 'catalog.json'), $snapshot + [Environment]::NewLine, $encoding)
[IO.File]::WriteAllText((Join-Path $target 'provenance.json'), $provenance + [Environment]::NewLine, $encoding)
[IO.File]::WriteAllText((Join-Path $target 'LICENSE'), $license, $encoding)
Write-Output "Updated models.dev snapshot ($($providers.Count) providers, $($snapshot.Length) characters)"
