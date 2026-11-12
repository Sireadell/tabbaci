# Hosting Tabbaci demo

## What is ready now

| Item | State |
|---|---|
| Build script (`tools/host/build.mjs`) | Ready. Copies the site into `dist/` with no server needed. |
| Smoke test (`tools/host/smoke.mjs`) | Ready. Passes locally: 17 of 17 checks, including the NEAR testnet call. |
| Anything put online | Not done. Nothing is uploaded, signed up or published. |

## What the founder must approve before hosting

| # | Decision | Why it matters |
|---|---|---|
| 1 | Which free host (for example Netlify Drop, Cloudflare Pages or GitHub Pages) | Each host has its own sign-up and its own limits. |
| 2 | Whether to create a new account on that host, and under which email | Creating an account is a step only you can approve. |
| 3 | The public web address the demo will get | Anyone with the link can open it. |
| 4 | What becomes public | The site files in `dist/` only: the three pages, the style and the list of pages. No keys, no `.env`, no research folders. The site reads the NEAR testnet chain straight from the visitor's browser. |
| 5 | Whether the GitHub repo goes public | Not needed for a static upload. Keep it private unless you choose otherwise. |

## Commands for later (PowerShell 5.1, not run yet)

Run these only after you approve the steps above.

```powershell
cd C:\Users\DELL\hackathons\near\enc-hosting
node tools\host\build.mjs
node tools\host\smoke.mjs
Compress-Archive -Path dist\* -DestinationPath C:\Users\DELL\Desktop\encyclopedia-site.zip -Force
```

What each one does:

| Command | Plain meaning |
|---|---|
| `node tools\host\build.mjs` | Rebuilds the `dist` folder from the site files. |
| `node tools\host\smoke.mjs` | Checks the built copy locally and checks the NEAR test network answers. Exit code 0 means pass. |
| `Compress-Archive ...` | Packs the built folder into one zip on your Desktop, ready to drag into a free host's upload page. |

The upload itself is a separate step. It needs your approval for the chosen host.
