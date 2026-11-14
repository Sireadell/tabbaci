# Lock BOTH secrets of the private court in one OutLayer secret: the NEAR AI key (for the judges) and the
# founder's read-only GitHub token (for the private source). Locked to one program build and one contract,
# then every key on the owner account is deleted. Testnet. Run it on your own machine, in your own terminal.
#
# Why one script does both: OutLayer gives a run ONE secrets profile of ONE owner account, so both values must sit
# in the same secret. This script reads the AI key from its file itself and asks you for the GitHub token with a
# hidden prompt, so nobody else sees the token and you never have to look at the AI key.
#
#   .\lock-private.ps1 -Owner pkey.sireadell.testnet -ProgramHash <sha256 of the real court-private build> `
#                      -Via <courtroom-private contract> -Create [-Repo sireadell/private-repo]
#
# -Via       only calls that come through this contract may read the secret
# -Create    create the owner account first (funded by -Funder, 0.16 NEAR)
# -Profile   name of the secret row, default "court". The contract's secrets_account is -Owner, secrets_profile is this.
# -Repo      optional: before storing, check the token can read this repo (one request to api.github.com)
# -SourceEnv the secret name the private source uses, default PRIVATE_SOURCE_KEY
#
# Prints only lengths and the final LOCKED line. Never prints, logs or saves either value.
param(
  [Parameter(Mandatory)][string]$Owner,
  [Parameter(Mandatory)][string]$ProgramHash,
  [Parameter(Mandatory)][string]$Via,
  [string]$AiKeyFile = 'C:\Users\DELL\.near-secrets\nearai-api-key.txt',
  [string]$Profile = 'court',
  [string]$SourceEnv = 'PRIVATE_SOURCE_KEY',
  [string]$Repo = '',
  [string]$Funder = 'sireadell.testnet',
  [string]$Fund = '0.16 NEAR',
  [switch]$Create
)
$ErrorActionPreference = 'Stop'
$near = 'C:\Users\DELL\near-tools\bin\near.exe'
$outlayer = 'C:\Users\DELL\near-tools\bin\outlayer.exe'
$userHome = 'C:\Users\DELL'
$cred = "$userHome\.near-credentials\testnet\$Owner.json"
if ($ProgramHash -notmatch '^[0-9a-f]{64}$') { throw 'ProgramHash must be 64 lowercase hex characters' }
if ($SourceEnv -cnotmatch '^PRIVATE_[A-Z0-9_]{1,40}$') { throw 'SourceEnv must be PRIVATE_ followed by capitals, digits or _' }
if (-not (Test-Path $AiKeyFile)) { throw "no AI key file at $AiKeyFile" }

# 1. The two values, held in memory only.
$ai = (Get-Content $AiKeyFile -Raw).Trim()
if ($ai.Length -lt 8) { throw 'the AI key file looks empty' }
$secure = Read-Host -AsSecureString 'Paste the read-only GitHub token (nothing will show)'
$bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
try { $gh = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr).Trim() } finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr) }
$secure.Dispose()
if ($gh.Length -lt 20 -or $gh -notmatch '^(github_pat_|ghp_)[A-Za-z0-9_]+$') { $gh = $null; throw 'that does not look like a GitHub token (github_pat_... or ghp_...)' }
$aiLen = $ai.Length; $ghLen = $gh.Length
"read: NEAR_AI_KEY length $aiLen, $SourceEnv length $ghLen"

if ($Repo) {
  if ($Repo -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') { throw 'Repo must look like owner/name' }
  try {
    $resp = Invoke-WebRequest -UseBasicParsing -Uri "https://api.github.com/repos/$Repo" -Headers @{ Authorization = "Bearer $gh"; 'User-Agent' = 'courtroom-lock'; Accept = 'application/json' }
    $priv = ($resp.Content | ConvertFrom-Json).private
    "token reads $Repo : yes (private = $priv)"
  } catch {
    $gh = $null; $ai = $null
    throw "the token cannot read $Repo (GitHub said: $($_.Exception.Response.StatusCode))"
  }
}

# 2. The owner account.
if ($Create) {
  & $near account create-account fund-myself $Owner $Fund autogenerate-new-keypair save-to-legacy-keychain sign-as $Funder network-config testnet sign-with-legacy-keychain send | Out-Null
  if (-not (Test-Path $cred)) { throw "account was not created: $Owner" }
}
if (-not (Test-Path $cred)) { throw "no saved key for $Owner (use -Create)" }
$c = Get-Content $cred -Raw | ConvertFrom-Json

# 3. Store both values as one secret. OutLayer keeps one login at a time in ~\.outlayer, so swap it for this one
#    command and always restore it.
$main = "$userHome\.outlayer"; $bak = "$userHome\.outlayer-main-backup"
Copy-Item $main $bak -Recurse -Force
try {
  $in = Join-Path $env:TEMP ("ol-" + [guid]::NewGuid().ToString('N') + '.txt')
  [IO.File]::WriteAllText($in, "$Owner`n$($c.private_key)`n")
  Start-Process $outlayer -ArgumentList 'login','testnet' -NoNewWindow -Wait -RedirectStandardInput $in -RedirectStandardOutput "$env:TEMP\ol-o.txt" -RedirectStandardError "$env:TEMP\ol-e.txt"
  Remove-Item $in -Force
  Remove-Item "$env:TEMP\ol-o.txt","$env:TEMP\ol-e.txt" -Force -ErrorAction SilentlyContinue
  $me = ((& $outlayer whoami | Select-String 'Account:\s*(\S+)').Matches[0].Groups[1].Value)
  if ($me -ne $Owner) { throw "OutLayer login is $me, expected $Owner" }

  $esc = { param($s) $s.Replace('\','\\').Replace('"','\"') }
  $json = '{"NEAR_AI_KEY":"' + (& $esc $ai) + '","' + $SourceEnv + '":"' + (& $esc $gh) + '"}'
  $json = $json.Replace('"','\"')   # PowerShell 5.1 drops inner quotes of native arguments unless escaped
  $ErrorActionPreference = 'Continue'   # PowerShell 5.1 turns any stderr line (even a plain notice) into an error otherwise
  & $outlayer secrets set $json --profile $Profile --wasm-hash $ProgramHash --access allow-all --via $Via 2>&1 | Out-Null
  $code = $LASTEXITCODE
  $ErrorActionPreference = 'Stop'
  $json = $null; $gh = $null; $ai = $null
  if ($code -ne 0) { throw 'storing the secret failed' }
  # what is stored, names only (OutLayer lists metadata, never values)
  $ErrorActionPreference = 'Continue'
  $listed = (& $outlayer secrets list 2>&1 | Out-String)
  $ErrorActionPreference = 'Stop'
  $seen = if ($listed -match [regex]::Escape($Profile)) { 'yes' } else { 'not shown' }
  "secret row $Profile listed: $seen"
} finally {
  $json = $null; $gh = $null; $ai = $null
  Copy-Item "$bak\testnet\credentials.json" "$main\testnet\credentials.json" -Force
  Remove-Item $bak -Recurse -Force
}

# 4. The hole: if the owner keeps a key it can widen who may read. Delete them all, then check.
& $near account delete-keys $Owner public-keys $c.public_key network-config testnet sign-with-legacy-keychain send | Out-Null
Start-Sleep 4
$r = Invoke-RestMethod -Uri https://rpc.testnet.near.org -Method Post -ContentType 'application/json' -Body (@{jsonrpc='2.0';id=1;method='query';params=@{request_type='view_access_key_list';finality='final';account_id=$Owner}} | ConvertTo-Json -Depth 5)
$n = @($r.result.keys).Count
if ($n -ne 0) { throw "$Owner still has $n key(s)" }
Remove-Item $cred -Force   # the saved private key no longer opens anything
"LOCKED owner=$Owner profile=$Profile program=$ProgramHash via=$Via NEAR_AI_KEY_len=$aiLen ${SourceEnv}_len=$ghLen keys_left=0"
