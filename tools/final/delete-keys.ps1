# Delete every access key on the final contract account, so nobody (including us) can change or drain it.
# NOT reversible. Run it only after the checklist in docs/REAL_JUDGE_SWITCH.md is done.
# Windows PowerShell 5.1.
#
#   .\delete-keys.ps1 -Contract final1.sireadell.testnet            (shows what it would delete, deletes nothing)
#   .\delete-keys.ps1 -Contract final1.sireadell.testnet -Yes       (deletes)
#   add -Network mainnet for the mainnet contract
param(
  [Parameter(Mandatory)][string]$Contract,
  [ValidateSet('testnet','mainnet')][string]$Network = 'testnet',
  [switch]$Yes
)
$ErrorActionPreference = 'Stop'
$near = 'C:\Users\DELL\near-tools\bin\near.exe'
$rpc = if ($Network -eq 'mainnet') { 'https://rpc.mainnet.near.org' } else { 'https://rpc.testnet.near.org' }

function Get-Keys {
  $body = @{jsonrpc='2.0';id=1;method='query';params=@{request_type='view_access_key_list';finality='final';account_id=$Contract}} | ConvertTo-Json -Depth 5
  $r = Invoke-RestMethod -Uri $rpc -Method Post -ContentType 'application/json' -Body $body
  if ($r.error) { throw "RPC error: $($r.error.data)" }
  return @($r.result.keys)
}

$keys = Get-Keys
if ($keys.Count -eq 0) { "$Contract already has 0 keys."; exit 0 }
"$Contract has $($keys.Count) key(s):"
$keys | ForEach-Object { "  " + $_.public_key }
if (-not $Yes) { "Dry run. Nothing deleted. Add -Yes to delete them all (cannot be undone)."; exit 0 }

$list = ($keys | ForEach-Object { $_.public_key }) -join ','
& $near account delete-keys $Contract public-keys $list network-config $Network sign-with-legacy-keychain send | Out-Null
Start-Sleep 5
$left = (Get-Keys).Count
if ($left -ne 0) { throw "$Contract still has $left key(s)" }
# the saved private key opens nothing now
$cred = "C:\Users\DELL\.near-credentials\$Network\$Contract.json"
if (Test-Path $cred) { Remove-Item $cred -Force }
$cred2 = "C:\Users\DELL\.near-credentials\$Network\$Contract"
if (Test-Path $cred2) { Remove-Item $cred2 -Recurse -Force }
"DONE $Contract keys_left=0"
"Check: node tools/verify/verify.mjs --contract $Contract --round <id> --network $Network  (the 'contract has no keys' line should now say MATCH)"
