"""Read-only release gate. Python 3.11+, no third-party dependencies."""
import argparse
import json
from pathlib import Path
import re
import tomllib


def check(root: Path, version: str, product_name: str = 'product') -> None:
    if not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+', version):
        raise ValueError('expected numeric major.minor.patch')
    product = root / product_name
    manifest = json.loads((product / 'components.json').read_text(encoding='utf-8'))
    core = tomllib.loads((root / 'localreel-core/Cargo.toml').read_text(encoding='utf-8'))
    versions = {
        'product': manifest['version'],
        'core pin label': manifest['components']['core']['version'],
        'web pin label': manifest['components']['web']['version'],
        'core workspace': core['workspace']['package']['version'],
    }
    for name, directory in [('web', root / 'localreel-web'), ('desktop', product / 'desktop')]:
        package = json.loads((directory / 'package.json').read_text(encoding='utf-8'))
        lock = json.loads((directory / 'package-lock.json').read_text(encoding='utf-8'))
        versions[name] = package['version']
        versions[name + ' lock'] = lock['version']
        versions[name + ' lock root'] = lock['packages']['']['version']
    errors = [f'{name}: {actual} != {version}' for name, actual in versions.items() if actual != version]
    if errors:
        raise ValueError('release version mismatch: ' + '; '.join(errors))
    print(f'All {len(versions)} version fields match {version}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--version', required=True)
    parser.add_argument('--product-dir', default='product')
    args = parser.parse_args()
    check(args.root, args.version, args.product_dir)
