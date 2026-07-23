<#
.SYNOPSIS
  One-command release for Archiplayer: bump version, commit, tag, and let CI build
  and publish the GitHub release.

.DESCRIPTION
  Drives the existing .github/workflows/release.yml pipeline. The workflow triggers
  on pushing a vX.Y.Z tag, builds Windows/macOS/Linux via tauri-action into a DRAFT
  release, then generates SHA256SUMS.txt. This script performs the manual half
  (version bump, commit, tag push), watches the run, verifies all assets landed, and
  publishes the draft.

  NEVER use `gh release create` for a release: it creates the tag AND pre-publishes,
  which collides with what CI does. The tag push is the only trigger.

.PARAMETER Version
  Semantic version X.Y.Z (no leading v). The tag pushed is v<Version>.

.PARAMETER NoPublish
  Push the tag and watch CI, but leave the release as a draft for manual review.

.PARAMETER NoWatch
  Push the tag and exit immediately without watching the CI run (implies -NoPublish,
  since publishing requires a completed, verified run).

.PARAMETER Yes
  Non-interactive: skip the confirmation prompt before the tag push.

.EXAMPLE
  pwsh -File scripts/release.ps1 -Version 0.4.0
  Full auto: bump, commit, push, watch CI, verify assets, publish.

.EXAMPLE
  pwsh -File scripts/release.ps1 -Version 0.4.0 -WhatIf
  Dry run: print every mutating step, change nothing.
#>
[CmdletBinding(SupportsShouldProcess)]
param(
  [Parameter(Mandatory)]
  [ValidatePattern('^\d+\.\d+\.\d+$')]
  [string]$Version,

  [switch]$NoPublish,
  [switch]$NoWatch,
  [switch]$Yes
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $repoRoot

$tag = "v$Version"

# --- helpers ----------------------------------------------------------------

function Invoke-Native {
  param([Parameter(Mandatory)][string]$File, [Parameter(ValueFromRemainingArguments)][string[]]$Arguments)
  & $File @Arguments
  if ($LASTEXITCODE -ne 0) { throw "$File $($Arguments -join ' ') failed (exit $LASTEXITCODE)." }
}

# Anchored, minimal-diff version bump. Reads/writes raw bytes so JSON is not
# reflowed and no BOM is introduced (Set-Content -Encoding utf8 adds a BOM on
# Windows PowerShell 5.1 and corrupts JSON). Replaces only the first match; the
# capture groups bracket the version literal so only it changes.
function Update-VersionFile {
  param([string]$RelPath, [string]$Pattern, [string]$NewVersion)
  $full = Join-Path $repoRoot $RelPath
  $raw = [System.IO.File]::ReadAllText($full)
  $re = [regex]$Pattern
  $m = $re.Match($raw)
  if (-not $m.Success) { throw "Version pattern not found in $RelPath (unexpected file shape)." }
  $replacement = $m.Groups[1].Value + $NewVersion + $m.Groups[2].Value
  if ($replacement -eq $m.Value) {
    Write-Host "  $RelPath already at $NewVersion"
    return
  }
  if ($PSCmdlet.ShouldProcess($RelPath, "bump version to $NewVersion")) {
    $out = $re.Replace($raw, { param($x) $x.Groups[1].Value + $NewVersion + $x.Groups[2].Value }, 1)
    [System.IO.File]::WriteAllText($full, $out, [System.Text.UTF8Encoding]::new($false))
    Write-Host "  bumped $RelPath -> $NewVersion"
  }
}

function Get-RepoSlug {
  $slug = & gh repo view --json nameWithOwner --jq .nameWithOwner
  if ($LASTEXITCODE -ne 0 -or -not $slug) { throw "Could not resolve the GitHub repo (is gh authenticated?)." }
  return $slug.Trim()
}

# Resolve a release by tag via the list endpoint. `gh release view <tag>` and
# /releases/tags/{tag} do not reliably return DRAFT releases, so match tag_name
# across all releases (the same approach the checksums CI job uses).
function Get-ReleaseByTag {
  param([string]$Repo, [string]$Tag)
  $json = & gh api "repos/$Repo/releases" --paginate --jq "map(select(.tag_name == `"$Tag`")) | .[0]"
  if ($LASTEXITCODE -ne 0) { throw "gh api releases failed for $Repo." }
  if (-not $json -or $json -eq "null") { return $null }
  return ($json | ConvertFrom-Json)
}

# --- steps ------------------------------------------------------------------

function Assert-Preconditions {
  Write-Host "Checking preconditions..."

  $branch = (& git rev-parse --abbrev-ref HEAD).Trim()
  if ($branch -ne "main") { throw "Not on main (on '$branch'). Releases are cut from main." }

  # Only tracked changes block a release; untracked scratch files are fine.
  $dirty = & git status --porcelain --untracked-files=no
  if ($dirty) { throw "Working tree has uncommitted changes to tracked files. Commit or stash first." }

  Invoke-Native git fetch origin --tags
  $head = (& git rev-parse HEAD).Trim()
  $remote = (& git rev-parse origin/main).Trim()
  if ($head -ne $remote) { throw "Local main ($head) is not level with origin/main ($remote). Pull/push first." }

  Invoke-Native gh auth status

  if (& git tag --list $tag) { throw "Local tag $tag already exists." }
  if (& git ls-remote --tags origin "refs/tags/$tag") {
    throw "Remote tag $tag already exists (CI may have already run for it)."
  }

  Write-Host "  ok: on main, clean, level with origin, authenticated, $tag is free."
}

function Update-AllVersions {
  Write-Host "Bumping version to $Version..."

  # package.json + package-lock.json (root and packages[""]) in one npm call.
  if ($PSCmdlet.ShouldProcess("package.json + package-lock.json", "npm version $Version")) {
    Invoke-Native npm version $Version --no-git-tag-version --allow-same-version | Out-Null
  }

  Update-VersionFile "src-tauri/tauri.conf.json" '("productName":\s*"Archiplayer",\s*"version":\s*")[^"]*(")' $Version
  Update-VersionFile "src-tauri/Cargo.toml"      '(?m)(^version = ")[^"]*(")'                                  $Version
  # Cargo.lock: several packages share a version, so anchor to the archiplayer entry.
  Update-VersionFile "src-tauri/Cargo.lock"      '(name = "archiplayer"\r?\nversion = ")[^"]*(")'              $Version
}

function Invoke-VersionGate {
  Write-Host "Verifying version consistency..."
  Invoke-Native node scripts/verify-release-version.mjs $tag
}

function New-ReleaseCommit {
  $paths = @(
    "package.json",
    "package-lock.json",
    "src-tauri/tauri.conf.json",
    "src-tauri/Cargo.toml",
    "src-tauri/Cargo.lock"
  )

  $lastSubject = (& git log -1 --pretty=%s).Trim()
  $pending = & git status --porcelain --untracked-files=no -- @paths
  if (-not $pending -and $lastSubject -eq "release: $tag") {
    Write-Host "  commit 'release: $tag' already exists; skipping."
    return
  }

  if ($PSCmdlet.ShouldProcess("release: $tag", "git commit version files")) {
    Invoke-Native git add -- @paths
    Invoke-Native git commit -m "release: $tag"
    Write-Host "  committed 'release: $tag'."
  }
}

function Push-Main {
  # Pushing main does NOT trigger the release workflow (it only listens on tags).
  if ($PSCmdlet.ShouldProcess("origin/main", "git push")) {
    Invoke-Native git push origin main
  }
}

function Confirm-TagPush {
  if ($Yes -or $WhatIfPreference) { return }
  Write-Host ""
  Write-Host "Pushing $tag triggers CI and creates a PUBLIC release. This is the point of no return." -ForegroundColor Yellow
  $answer = Read-Host "Type the tag ($tag) to continue, anything else to abort"
  if ($answer -ne $tag) { throw "Aborted before tag push." }
}

function New-AndPushTag {
  if ($PSCmdlet.ShouldProcess($tag, "create annotated tag and push (triggers CI)")) {
    # Reuse an existing correct local tag if a prior run got this far.
    $existing = (& git tag --list $tag)
    if ($existing) {
      $tagCommit = (& git rev-list -n 1 $tag).Trim()
      $head = (& git rev-parse HEAD).Trim()
      if ($tagCommit -ne $head) { throw "Local tag $tag points at $tagCommit, not HEAD ($head). Resolve manually." }
    } else {
      Invoke-Native git tag -a $tag -m "Archiplayer $tag"
    }
    Invoke-Native git push origin $tag
    Write-Host "  pushed $tag. CI is now building."
  }
}

function Wait-ForReleaseRun {
  if ($WhatIfPreference) { return }
  Write-Host "Locating the release workflow run..."
  $sha = (& git rev-parse HEAD).Trim()
  $runId = $null
  for ($i = 0; $i -lt 12; $i++) {
    $runs = & gh run list --workflow release.yml --limit 20 --json databaseId,headSha,event | ConvertFrom-Json
    $run = $runs | Where-Object { $_.headSha -eq $sha -and $_.event -eq "push" } | Select-Object -First 1
    if ($run) { $runId = $run.databaseId; break }
    Start-Sleep -Seconds 5
  }
  if (-not $runId) { throw "Could not find a release run for $sha. Check: gh run list --workflow release.yml" }

  Write-Host "  watching run $runId (this takes ~10-15 min for all platforms)..."
  # --exit-status makes gh return non-zero if the run concluded in failure.
  Invoke-Native gh run watch $runId --exit-status
}

function Assert-DraftAssets {
  param([string]$Repo)
  Write-Host "Verifying release assets..."
  $rel = Get-ReleaseByTag -Repo $Repo -Tag $tag
  if (-not $rel) { throw "No release found for $tag after CI. Aborting." }

  $names = @($rel.assets | ForEach-Object { $_.name })
  $required = @('_x64-setup.exe', '_universal.dmg', '_amd64.AppImage', '_amd64.deb', 'SHA256SUMS.txt')
  $missing = @()
  foreach ($needle in $required) {
    if (-not ($names | Where-Object { $_ -like "*$needle" })) { $missing += $needle }
  }
  if ($missing.Count -gt 0) {
    throw "Release $tag is missing assets: $($missing -join ', '). A platform build likely failed (checksums is skipped when any leg fails). Do NOT publish; fix and re-tag."
  }
  Write-Host "  all 5 assets present: $($names -join ', ')"
  return $rel
}

function Publish-Draft {
  param([string]$Repo, $Release)
  if ($Release -and $Release.draft -eq $false) {
    Write-Host "  release $tag is already published."
    return
  }
  if ($PSCmdlet.ShouldProcess($tag, "publish release (draft -> published)")) {
    Invoke-Native gh api --method PATCH "repos/$Repo/releases/$($Release.id)" -F draft=false | Out-Null
    Write-Host "  published $tag. The site (releases/latest) will now reflect it."
  }
}

# --- orchestration ----------------------------------------------------------

Write-Host "=== Archiplayer release $tag ===" -ForegroundColor Cyan

Assert-Preconditions
Update-AllVersions
Invoke-VersionGate
New-ReleaseCommit
Push-Main
Confirm-TagPush
New-AndPushTag

if ($NoWatch) {
  Write-Host ""
  Write-Host "Tag pushed. CI is building. Not watching (-NoWatch)." -ForegroundColor Green
  Write-Host "When it finishes, verify assets and publish:"
  Write-Host "  gh run list --workflow release.yml"
  Write-Host "  gh release view $tag --json assets"
  Write-Host "  gh release edit $tag --draft=false"
  return
}

Wait-ForReleaseRun

$repo = Get-RepoSlug
$release = Assert-DraftAssets -Repo $repo

if ($NoPublish) {
  Write-Host ""
  Write-Host "CI succeeded and all assets are present, but -NoPublish was set." -ForegroundColor Green
  Write-Host "Review the draft, then publish with: gh release edit $tag --draft=false"
  return
}

Publish-Draft -Repo $repo -Release $release

Write-Host ""
Write-Host "=== $tag released ===" -ForegroundColor Green
Write-Host "Post-release: submit the installer to VirusTotal and drop the permalink into the site (data-vt-todo) and release notes."
