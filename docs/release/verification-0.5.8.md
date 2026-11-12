# 0.5.8 release verification preparation

Status: source and local artifacts verified, **real setup/Sandbox installation has
not been executed**. Do not infer release readiness from source tests or an npm
dependency version.

## Verified so far (2026-11-11)

| Item | Evidence |
| --- | --- |
| Component CI | core / web / plugin-cli / plugin-strm `ci` green on GitHub for the pinned commits |
| Product CI | `safety` workflow green (desktop tests, uninstall regression, installer tests) |
| Local server candidate | `--version` prints 0.5.8; built from pinned sources with `--locked` |
| Electron runtime | official 44.9.1 archive, SHA256 matches the published `SHASUMS256.txt` |
| Desktop package | `electron-builder --dir` from that runtime: exe 0.5.8, packaged app reports Electron 44.9.1 |
| Desktop setup candidate | built and hashed locally; unsigned |
| Version gate | all ten fields match 0.5.8 (product, pins, core workspace, web/desktop package+lock) |
| Signature gate | `test-release-gates.py` passes for correct key, wrong key and tampered manifest |

Not verified: the packaged installer was never executed, the Windows/GUI path was
not launched, macOS/Linux installers were not built locally, and the GitHub
release job (signing key, publishing) has not run.

Candidate artifacts rebuilt from the pinned commits (working tree clean):

| Artifact | Size | SHA256 |
| --- | --- | --- |
| `localreel.exe` (server) | 16,737,792 | `E6A06D4838123C0D413B735109FAEBD8B105618FB024CBB0C87C0ADEF8DBD60B` |
| desktop `LocalReel.exe` | 246,588,928 | `2535E0AF7D1F8976111CC58CEAE34D1DDB49D9FB0A4FDB56163E38CF20C49238` |
| desktop setup candidate | 153,199,616 | `0F076478202C8C438147E0A837A372139212A2B31961A3C38A0B831E3FF29BA9` |

The packaged tree reports Electron `44.9.1` (its `version` file and a packaged
executable probe agree); the desktop shell hash matched an earlier independent
build, so the packaging step is reproducible. The setup executable is unsigned.

## Version and source gate

Synchronize product/core/web/desktop to 0.5.8, including npm lock root versions
and the core Cargo.lock workspace entries. Installer crate version 0.1.0 is
independent: its product version comes from LOCALREEL_VERSION during packaging.
Commit component changes only after approval, then pin their actual commits in
components.json. Existing pins intentionally remain unchanged until that step;
a dirty working tree is not represented by its HEAD hash.

From the workspace root (Python 3.11+):

```powershell
python LocalReel/scripts/check-release-versions.py --root . --product-dir LocalReel --version 0.5.8
```

The release workflow performs the same check against checked-out pinned commits
before any platform job. Build without SkipBuild or Force. Signing, checksums
and manifests must refer to the final bytes.

## Signing readiness (names only; never print secret values)

The existing GitHub release configuration uses UPDATE_SIGNING_KEY (Ed25519 PEM,
matching the compiled core update public key), COMPONENTS_TOKEN (private
component repository checkout; falls back to github.token), and GITHUB_TOKEN
(ephemeral GitHub release/GHCR permissions). No existing Authenticode PFX,
Developer ID or Apple notarization secrets are wired into the workflow.
Windows desktop signing is disabled; macOS native bundle is ad-hoc signed.
Retaining the existing signing configuration does not imply trusted OS signing.

Read metadata, not values:

```powershell
gh secret list --repo LocalReel/LocalReel --json name,updatedAt
gh run list --repo LocalReel/LocalReel --workflow release.yml --limit 5
```

Organization secrets may require an administrator to verify that this repository
has access. Presence alone cannot prove the signing key matches. The release job checks out
the exact pinned core revision and verifies the generated signature against its
compiled public key before publishing. Private-key temporary files are protected
with umask 077 and removed by an EXIT trap, including failure paths.
`python -B scripts/test-release-gates.py` exercises both workspace layouts and
uses generated temporary Ed25519 keys to test correct/wrong-key/tampered-manifest
verification without any real signing configuration. Never run
scripts/sign.ps1 -SelfSignedTest on the host: it touches CurrentUser certificates.

## Actual installer test: disposable Windows Sandbox only

**Never run setup on the host, even with /D pointing at a scratch directory.**
It writes a fixed HKCU uninstall GUID and uses real known-folder shortcuts.
Changing APPDATA/LOCALAPPDATA/USERPROFILE alone does not isolate those operations.

Create a dedicated host directory containing only the candidate setup, optional
trusted previous setup, and scripts/test-setup-sandbox.ps1. Do not map the whole
workspace, credentials or a writable host directory. A .wsb configuration can use:

```xml
<Configuration>
  <Networking>Disable</Networking>
  <ClipboardRedirection>Disable</ClipboardRedirection>
  <MappedFolders>
    <MappedFolder>
      <HostFolder>C:\path\to\dedicated-payload</HostFolder>
      <SandboxFolder>C:\payload</SandboxFolder>
      <ReadOnly>true</ReadOnly>
    </MappedFolder>
  </MappedFolders>
</Configuration>
```

Inside the guest, in Windows PowerShell:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File C:\payload\test-setup-sandbox.ps1 -Installer C:\payload\candidate-setup.exe -ExpectedVersion 0.5.8 -ConfirmDisposableSandbox
# In a NEW guest, test an actual previous-version upgrade when available:
powershell -NoProfile -ExecutionPolicy Bypass -File C:\payload\test-setup-sandbox.ps1 -Installer C:\payload\candidate-setup.exe -PreviousInstaller C:\payload\previous-setup.exe -ExpectedVersion 0.5.8 -ConfirmDisposableSandbox
```

The script defaults to refusing execution unless the real current account and
profile match Windows Sandbox's WDAGUtilityAccount and the explicit switch is
present. This is an accident-prevention guard, not a security boundary against
spoofing. Ordinary VM users are deliberately refused; do not bypass the guard on
the host. Use a fresh Sandbox or an independently reviewed VM-specific harness.
The script installs with /nolaunch, writes dummy custom-library/config sentinels,
upgrades, verifies the v2 payload manifest and registration, uninstalls without
/removedata, checks sentinel SHA256 hashes and removal of registry/shortcuts,
and retains guest evidence until the Sandbox closes. It does not start a server.

Host-safe tests remain scripts/test-uninstall.ps1 (redirected shortcut/appdata
paths) and installer Rust unit tests (pure extraction/removal logic). They do not
replace actual disposable-guest integration coverage. Push/tag/release requires
separate final confirmation.
