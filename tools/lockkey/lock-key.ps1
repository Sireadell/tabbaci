# Lock the NEAR AI key to one program build, then remove every key from the account that owns the secret.
# Testnet. Never prints the key. Run it again with a NEW owner account whenever the program build changes,
# because after the last step nobody (including us) can change who may read the secret.
#
#   .\lock-key.ps1 -Owner key.sireadell.testnet -ProgramHash <sha256 of the real courtroom build> `
#                  -KeyFile C:\Users\DELL\.near-secrets\near-ai-key.txt -Via <final courtroom contract> -Create
#
# -Via     only calls that come through this contract may read the key (stops anyone else spending our credit)
# -Create  create the owner account first (funded by sireadell.testnet, 0.16 NEAR)
# -Profile name of the secret row, default "judge". The contract's secrets_account is -Owner, secrets_profile is this.
param(
  [Parameter(Mandatory)][string]$Owner,
  [Parameter(Mandatory)][string]$ProgramHash,
  [Parameter(Mandatory)][string]$KeyFile,
  [string]$Via = '',
  [string]$Profile = 'judge',
  [string]$Funder = 'sireadell.testnet',
  [string]$Fund = '0.16 NEAR',
  [switch]$Create,
  [string]$EnvName = 'NEAR_AI_KEY'
)
$ErrorActionPreference = 'Stop'
$near = 'C:\Users\DELL\near-tools\bin\near.exe'
$userHome = 'C:\Users\DELL'
$cred = "$userHome\.near-credentials\testnet\$Owner.json"
if ($ProgramHash -notmatch '^[0-9a-f]{64}$') { throw 'ProgramHash must be 64 lowercase hex characters' }

if ($Create) {
  & $near account create-account fund-myself $Owner $Fund autogenerate-new-keypair save-to-legacy-keychain sign-as $Funder network-config testnet sign-with-legacy-keychain send | Out-Null
  if (-not (Test-Path $cred)) { throw "account was not created: $Owner" }
}
if (-not (Test-Path $cred)) { throw "no saved key for $Owner (use -Create)" }
$c = Get-Content $cred -Raw | ConvertFrom-Json

# OutLayer keeps one login at a time in ~\.outlayer, so swap it for this one command and always restore it.
$main = "$userHome\.outlayer"; $bak = "$userHome\.outlayer-main-backup"
Copy-Item $main $bak -Recurse -Force
try {
  $in = Join-Path $env:TEMP ("ol-" + [guid]::NewGuid().ToString('N') + '.txt')
  [IO.File]::WriteAllText($in, "$Owner`n$($c.private_key)`n")
  Start-Process outlayer -ArgumentList 'login','testnet' -NoNewWindow -Wait -RedirectStandardInput $in -RedirectStandardOutput "$env:TEMP\ol-o.txt" -RedirectStandardError "$env:TEMP\ol-e.txt"
  Remove-Item $in -Force
  $me = ((& outlayer whoami | Select-String 'Account:\s*(\S+)').Matches[0].Groups[1].Value)
  if ($me -ne $Owner) { throw "OutLayer login is $me, expected $Owner" }

  $value = (Get-Content $KeyFile -Raw).Trim()
  $json = '{"' + $EnvName + '":"' + $value.Replace('\','\\').Replace('"','\"') + '"}'
  $json = $json.Replace('"','\"')   # PowerShell 5.1 drops inner quotes of native arguments unless escaped
  $acc = @('--access','allow-all')
  if ($Via) { $acc += @('--via',$Via) }
  $ErrorActionPreference = 'Continue'   # PowerShell 5.1 turns any stderr line (even a plain notice) into an error otherwise
  & outlayer secrets set $json --profile $Profile --wasm-hash $ProgramHash @acc 2>&1 | Out-Null
  $ErrorActionPreference = 'Stop'
  if ($LASTEXITCODE -ne 0) { throw 'storing the secret failed' }
  $value = $null; $json = $null
} finally {
  Copy-Item "$bak\testnet\credentials.json" "$main\testnet\credentials.json" -Force
  Remove-Item $bak -Recurse -Force
}

# The hole: if the owner keeps a key it can widen who may read. Delete them all, then check.
& $near account delete-keys $Owner public-keys $c.public_key network-config testnet sign-with-legacy-keychain send | Out-Null
Start-Sleep 4
$r = Invoke-RestMethod -Uri https://rpc.testnet.near.org -Method Post -ContentType 'application/json' -Body (@{jsonrpc='2.0';id=1;method='query';params=@{request_type='view_access_key_list';finality='final';account_id=$Owner}} | ConvertTo-Json -Depth 5)
$n = @($r.result.keys).Count
if ($n -ne 0) { throw "$Owner still has $n key(s)" }
Remove-Item $cred -Force   # the saved private key no longer opens anything
$viaText = if ($Via) { $Via } else { '(none, hash lock only)' }
"LOCKED owner=$Owner profile=$Profile program=$ProgramHash via=$viaText keys_left=0"
