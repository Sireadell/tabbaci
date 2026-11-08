# Run a program on OutLayer testnet as the logged-in account and print the program's reply (or the failure) from the chain.
# usage: .\run-and-read.ps1 -Wasm <url> -Hash <sha256> [-Profile p -Owner acct] [-Input '{}']
param([string]$Wasm,[string]$Hash,[string]$Profile,[string]$Owner,[string]$Input='{}')
$a=@('run','--wasm',$Wasm,'--hash',$Hash)
if($Profile){ $a+=@('--secrets-profile',$Profile,'--secrets-account',$Owner) }
$a+=$Input
$out = (& outlayer @a 2>&1 | Out-String)
$m = [regex]::Match($out,'Tx:\s*(\w{40,50})')
if(-not $m.Success){ Write-Output ($out -replace '\s+',' ').Trim().Substring(0,[Math]::Min(300,$out.Length)); exit 2 }
$me = ((& outlayer whoami | Select-String 'Account:\s*(\S+)').Matches[0].Groups[1].Value)
$body = @{jsonrpc='2.0';id=1;method='EXPERIMENTAL_tx_status';params=@{tx_hash=$m.Groups[1].Value;sender_account_id=$me;wait_until='FINAL'}} | ConvertTo-Json -Depth 5
$j = Invoke-RestMethod -Uri https://archival-rpc.testnet.near.org -Method Post -ContentType 'application/json' -Body $body
foreach($o in $j.result.receipts_outcome){
  foreach($l in $o.outcome.logs){ if($l -like 'Execution failed*' -or $l -like 'Execution refused*'){ Write-Output $l } }
  $s=$o.outcome.status
  if($s.SuccessValue){ $t=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($s.SuccessValue)); if($t -and $t -ne 'null'){ Write-Output "REPLY $t" } }
}
Write-Output "TX $($m.Groups[1].Value)"
