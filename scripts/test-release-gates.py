"""Local ephemeral fixtures only; never reads real signing secrets."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

HERE = Path(__file__).parent

def load(name):
    spec = importlib.util.spec_from_file_location(name, HERE / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

versions = load('check-release-versions')
signatures = load('verify-update-signature')

class ReleaseTests(unittest.TestCase):
    def test_versions_both_layouts_and_mismatch(self):
        with tempfile.TemporaryDirectory(prefix='localreel-version-test-') as directory:
            root = Path(directory)
            for product in ('product', 'LocalReel'):
                manifest = {'version':'0.5.8','components':{'core':{'version':'0.5.8'},'web':{'version':'0.5.8'}}}
                (root / product).mkdir()
                (root / product / 'components.json').write_text(json.dumps(manifest))
                for package in (root / 'localreel-web', root / product / 'desktop'):
                    package.mkdir(exist_ok=True)
                    (package / 'package.json').write_text(json.dumps({'version':'0.5.8'}))
                    (package / 'package-lock.json').write_text(json.dumps({'version':'0.5.8','packages':{'':{'version':'0.5.8'}}}))
                (root / 'localreel-core').mkdir(exist_ok=True)
                cargo = root / 'localreel-core/Cargo.toml'
                cargo.write_text('[workspace.package]\nversion="0.5.8"\n')
                versions.check(root, '0.5.8', product)
                cargo.write_text('[workspace.package]\nversion="0.5.7"\n')
                with self.assertRaises(ValueError): versions.check(root, '0.5.8', product)
                cargo.write_text('[workspace.package]\nversion="0.5.8"\n')
                with self.assertRaises(ValueError): versions.check(root, '0.5.9', product)

    def test_signature_correct_wrong_key_and_tampering(self):
        with tempfile.TemporaryDirectory(prefix='localreel-sign-test-') as directory:
            root = Path(directory)
            manifest = root / 'update.json'
            manifest.write_bytes(b'{"version":"0.5.8"}\n')
            for name in ('correct', 'wrong'):
                subprocess.run(['openssl','genpkey','-algorithm','ED25519','-out',str(root / (name + '.pem'))], check=True)
                der = subprocess.check_output(['openssl','pkey','-in',str(root / (name + '.pem')),'-pubout','-outform','DER'])
                self.assertEqual(der[:12], signatures.PREFIX)
                self.assertEqual(len(der), 44)
                (root / (name + '.txt')).write_text(der[-32:].hex())
                self.assertEqual(signatures.public_der(der[-32:].hex()), der)
            signature = root / 'signature.bin'
            subprocess.run(['openssl','pkeyutl','-sign','-inkey',str(root / 'correct.pem'),'-rawin','-in',str(manifest),'-out',str(signature)], check=True)
            signatures.verify(root / 'correct.txt', manifest, signature)
            with self.assertRaises(subprocess.CalledProcessError): signatures.verify(root / 'wrong.txt', manifest, signature)
            manifest.write_bytes(b'{"version":"9.9.9"}')
            with self.assertRaises(subprocess.CalledProcessError): signatures.verify(root / 'correct.txt', manifest, signature)
            with self.assertRaises(ValueError): signatures.public_der('bad-key')

if __name__ == '__main__':
    unittest.main()
