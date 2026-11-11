"""Verify detached Ed25519 signatures with the exact core release public key."""
import argparse
from pathlib import Path
import re
import subprocess
import tempfile

PREFIX = bytes.fromhex('302a300506032b6570032100')


def public_der(text: str) -> bytes:
    key = text.strip()
    if not re.fullmatch(r'[0-9a-fA-F]{64}', key):
        raise ValueError('update public key must contain exactly 32 bytes of hex')
    return PREFIX + bytes.fromhex(key)


def verify(public_key: Path, manifest: Path, signature: Path) -> None:
    if len(signature.read_bytes()) != 64:
        raise ValueError('Ed25519 signature must be exactly 64 bytes')
    with tempfile.TemporaryDirectory(prefix='localreel-verify-public-') as directory:
        der = Path(directory) / 'public.der'
        der.write_bytes(public_der(public_key.read_text(encoding='utf-8')))
        subprocess.run(['openssl', 'pkeyutl', '-verify', '-pubin', '-keyform', 'DER',
                        '-inkey', str(der), '-rawin', '-in', str(manifest),
                        '-sigfile', str(signature)], check=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--public-key', type=Path, required=True)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--signature', type=Path, required=True)
    args = parser.parse_args()
    verify(args.public_key, args.manifest, args.signature)
