# Updating LocalReel

LocalReel can update itself from the console. The check is passive by default:
nothing is downloaded or installed until an administrator asks for it.

## What happens

1. **Check.** The console's Overview page asks `GET /api/v1/admin/update`. The
   server reads the latest release feed at most once per `interval_hours`
   (default 24) and reports the version, the release page and whether something
   newer exists.
2. **Download.** *Download and install* asks the server to fetch the signed
   manifest (`update.json`) and its detached signature (`update.json.sig`),
   verify both against the public key compiled into the binary, refuse a
   version that is not newer, and download the installer for this platform.
   HTTPS only; the file is written as `.part` and renamed only after the size
   and SHA-256 match the manifest.
3. **Install.** *Install and restart* hands the verified installer to
   `localreel-setup.exe` with `/S /wait /launch` and stops the server. The
   installer waits for the process to exit, replaces the program, and starts it
   again. The page reconnects once the server is back.

Nothing is installed in the background, no channel is switched, and no
downgrade is offered.

## Which installations can update themselves

| Installed as | Button | Why |
|---|---|---|
| Per-user setup (`%LOCALAPPDATA%\Programs\LocalReel`) | yes | no elevation needed, the setup replaces itself |
| MSI (`Program Files`) | no | replacing it requires an administrator; the card links to the release page |
| zip / tarball / build directory | no | there is no installer to hand over |
| Docker image, Synology package | no | update those with `docker pull` and the package centre |

The card always says which case applies, so a button is never shown that would
fail.

## Configuration

```toml
[update]
# Check automatically and show the card. Default: true.
check = true
# Release feed used for "is there something newer". Default: the public API.
# feed = "https://api.github.com/repos/LocalReel/LocalReel/releases/latest"
# How often the feed is read, in hours. Default: 24.
interval_hours = 24
# The signed manifest used by the download and install path. Default: the
# latest release asset. Point it at a mirror to keep updates on the LAN.
# manifest = "https://mirror.internal/localreel/update.json"
```

A mirror only serves bytes: the signature is verified against the key compiled
into the binary, so a mirror cannot forge a release.

## The signing key

`crates/localreel-update/pubkey.txt` holds the public half (hex) that is
compiled into every binary. The private half lives only in the
`UPDATE_SIGNING_KEY` secret of the release workflow.

To rotate:

```bash
# 1. A new key pair. Keep signing.pem out of the repository.
openssl genpkey -algorithm ED25519 -out signing.pem

# 2. The public half, as the 32 bytes at the end of the DER key.
openssl pkey -in signing.pem -pubout -outform DER -out public.der
tail -c 32 public.der | xxd -p -c 64     # paste into pubkey.txt

# 3. The private half, exactly as the workflow expects it (PEM text).
cat signing.pem                          # paste into the UPDATE_SIGNING_KEY secret
```

A release carries the new public key only after it is installed, so rotate by
shipping one release signed with the *old* key that already contains the new
public key, then switching the secret for the next one.

## Troubleshooting

- **"the signature does not match the manifest"** — the manifest was edited or
  signed with a different key than the one in this binary. Nothing is
  installed. Use the release page.
- **"this release has no build for windows-x64"** — the release did not attach
  an installer for this platform. Check the workflow log.
- **"this installation cannot update itself (system_msi)"** — download the new
  installer or MSI from the release page.
- **The install button stops at "installing"** — the setup replaces the program
  after the server exits; give it a minute and reload. If the new version does
  not start, run the new setup again by hand.
