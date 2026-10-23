function Invoke-RestMethod {
    param([string]$Uri)
    if ($env:MOCK_FAIL -eq 'view') { throw 'Simulated API failure' }
    if ($Uri -ne 'https://api.github.com/repos/zcaceres/easy-tix/releases/latest') { throw 'Wrong API URL' }
    return @{ tag_name = $env:MOCK_TAG }
}
function Invoke-WebRequest {
    param([switch]$UseBasicParsing, [string]$Uri, [string]$OutFile)
    if ($env:MOCK_FAIL -eq 'download') { throw 'Simulated download failure' }
    $prefix = "https://github.com/zcaceres/easy-tix/releases/download/$env:MOCK_TAG/"
    if (-not $Uri.StartsWith($prefix)) { throw 'Wrong download tag' }
    $name = $Uri.Substring($prefix.Length)
    Copy-Item -LiteralPath (Join-Path $env:MOCK_DOWNLOADS $name) -Destination $OutFile
}
