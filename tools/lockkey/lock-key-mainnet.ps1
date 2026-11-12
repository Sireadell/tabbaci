# Store the NEAR AI key as an OutLayer secret on MAINNET, readable only by one exact program build.
# Never prints the key. Deletes no keys: the owner account keeps its keys, so the lock is the hash only.
#
#   .\lock-key-mainnet.ps1 -Owner sireadell.near -ProgramHash <sha256> -KeyFile C:\Users\DELL\.near-secrets\nearai-api-key.txt
param(
  [Parameter(Mandatory)][string]$Owner,
  [Parameter(Mandatory)][string]$ProgramHash,
  [Parameter(Mandatory)][string]$KeyFile,
  [string]$Profile = 'judge',
  [string]$EnvName = 'NEAR_AI_KEY'
)
$ErrorActionPreference = 'Stop'
$outlayer = 'C:\Users\DELL\near-tools\bin\outlayer.exe'
$userHome = 'C:\Users\DELL'
$cred = "$userHome\.near-credentials\mainnet\$Owner.json"
if ($ProgramHash -notmatch '^[0-9a-f]{64}$') { throw 'ProgramHash must be 64 lowercase hex characters' }
if (-not (Test-Path $cred)) { throw "no saved key for $Owner" }
$c = Get-Content $cred -Raw | ConvertFrom-Json

# OutLayer keeps one login at a time in ~\.outlayer. Back the whole folder up and always restore it.
$main = "$userHome\.outlayer"; $bak = "$userHome\.outlayer-main-backup"
if (Test-Path $bak) { throw "$bak already exists, restore it first" }
Copy-Item $main $bak -Recurse -Force
try {
  $in = Join-Path $env:TEMP ("ol-" + [guid]::NewGuid().ToString('N') + '.txt')
  [IO.File]::WriteAllText($in, "$Owner`n$($c.private_key)`n")
  Start-Process $outlayer -ArgumentList 'login','mainnet' -NoNewWindow -Wait -RedirectStandardInput $in -RedirectStandardOutput "$env:TEMP\ol-o.txt" -RedirectStandardError "$env:TEMP\ol-e.txt"
  Remove-Item $in -Force
  $who = (& $outlayer whoami | Out-String)
  $me = [regex]::Match($who, 'Account:\s*(\S+)').Groups[1].Value
  if ($me -ne $Owner -or $who -notmatch 'mainnet') { throw "OutLayer login is $me, expected $Owner on mainnet" }

  $value = (Get-Content $KeyFile -Raw).Trim()
  $json = '{"' + $EnvName + '":"' + $value.Replace('\','\').Replace('"','\"') + '"}'
  $json = $json.Replace('"','\"')
  $ErrorActionPreference = 'Continue'
  & $outlayer secrets set $json --profile $Profile --wasm-hash $ProgramHash --access allow-all 2>&1 | Out-Null
  $code = $LASTEXITCODE
  $ErrorActionPreference = 'Stop'
  $value = $null; $json = $null
  if ($code -ne 0) { throw 'storing the secret failed' }
  & $outlayer secrets list 2>&1 | Out-String
} finally {
  Remove-Item $main -Recurse -Force
  Copy-Item $bak $main -Recurse -Force
  Remove-Item $bak -Recurse -Force
}
"STORED owner=$Owner profile=$Profile program=$ProgramHash (network mainnet, keys untouched)"
