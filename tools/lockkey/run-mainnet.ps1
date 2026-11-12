# Run the sealed court program once on OutLayer MAINNET as the owner account, with the stored judge secret.
# No contract, no bond, no fine. Saves the raw CLI output to -OutFile. Never prints a key.
#   .\run-mainnet.ps1 -Claim 'NEAR Protocol is a blockchain platform.' -Terms NEAR,blockchain -OutFile out.txt
param(
  [Parameter(Mandatory)][string]$Claim,
  [Parameter(Mandatory)][string[]]$Terms,
  [Parameter(Mandatory)][string]$OutFile,
  [int]$Round = 1,
  [string]$Owner = 'sireadell.near',
  [string]$Hash = '2c8e24a5fa6d2d2b60431207eeabddb3521b286936c78a82ca3bf99878e01f9c',
  [string]$Wasm = 'https://test.fastfs.io/sireadell.testnet/outlayer.testnet/2c8e24a5fa6d2d2b60431207eeabddb3521b286936c78a82ca3bf99878e01f9c.wasm'
)
$ErrorActionPreference = 'Stop'
$outlayer = 'C:\Users\DELL\near-tools\bin\outlayer.exe'
$userHome = 'C:\Users\DELL'
$c = Get-Content "$userHome\.near-credentials\mainnet\$Owner.json" -Raw | ConvertFrom-Json
$sha = [Security.Cryptography.SHA256]::Create()
$th = ($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($Claim)) | ForEach-Object { $_.ToString('x2') }) -join ''
$st = Invoke-RestMethod -Uri https://rpc.mainnet.near.org -Method Post -ContentType 'application/json' -Body '{"jsonrpc":"2.0","id":1,"method":"status","params":[]}'
$asof = [int64]$st.result.sync_info.latest_block_height
$obj = [ordered]@{
  v=1; rd=$Round; kind='prose'; th=$th; claim=$Claim
  src=@(
    [ordered]@{i=4;u='https://en.wikipedia.org/api/rest_v1/page/summary/NEAR_Protocol';x='';tol=0.0;g='wikipedia'},
    [ordered]@{i=5;u='https://raw.githubusercontent.com/near/nearcore/master/README.md';x='';tol=0.0;g='github'})
  m=@('deepseek-ai/DeepSeek-V4.1-Flash-1017','Qwen/Qwen3.8-27B','Qwen/Qwen3.6-35B-A3B-FP8')
  seed=(Get-Random -Maximum 4294967295); asof=$asof; terms=$Terms
}
$inputFile = Join-Path $env:TEMP ("court-in-" + [guid]::NewGuid().ToString('N') + '.json')
[IO.File]::WriteAllText($inputFile, ($obj | ConvertTo-Json -Depth 6 -Compress))

$main = "$userHome\.outlayer"; $bak = "$userHome\.outlayer-main-backup"
if (Test-Path $bak) { throw "$bak already exists, restore it first" }
Copy-Item $main $bak -Recurse -Force
try {
  $in = Join-Path $env:TEMP ("ol-" + [guid]::NewGuid().ToString('N') + '.txt')
  [IO.File]::WriteAllText($in, "$Owner`n$($c.private_key)`n")
  Start-Process $outlayer -ArgumentList 'login','mainnet' -NoNewWindow -Wait -RedirectStandardInput $in -RedirectStandardOutput "$env:TEMP\ol-o.txt" -RedirectStandardError "$env:TEMP\ol-e.txt"
  Remove-Item $in -Force
  $who = (& $outlayer whoami | Out-String)
  if ($who -notmatch [regex]::Escape($Owner) -or $who -notmatch 'mainnet') { throw 'OutLayer login is not the mainnet owner' }
  $ErrorActionPreference = 'Continue'
  $out = (& $outlayer run --wasm $Wasm --hash $Hash --secrets-profile judge --secrets-account $Owner --input $inputFile 2>&1 | Out-String)
  $ErrorActionPreference = 'Stop'
  [IO.File]::WriteAllText($OutFile, $out)
} finally {
  Remove-Item $inputFile -Force -ErrorAction SilentlyContinue
  Remove-Item $main -Recurse -Force
  Copy-Item $bak $main -Recurse -Force
  Remove-Item $bak -Recurse -Force
}
"input claim_hash=$th asof=$asof"
($out -split "`r?`n" | Select-Object -First 40) -join "`n"
