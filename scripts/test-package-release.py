"""Focused archive tests; no compiler, network or signing identity required."""
import importlib.util
from itertools import product
import json
from pathlib import Path
import plistlib
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

spec = importlib.util.spec_from_file_location('packager', Path(__file__).with_name('package-release.py'))
packager = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packager)


class NativeArchiveTests(unittest.TestCase):
    def test_all_targets_preserve_version_notices_source_identity_and_executable(self):
        for (target, suffix), version in product(packager.TARGETS.items(), ["1.2.3", "0.1.0-alpha.1"]):
            with self.subTest(target=target, version=version), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                (root / 'Cargo.toml').write_text(f'[package]\nversion = "{version}"\nrepository = "https://example.com/project"\n')
                for name in ['LICENSE', 'COPYRIGHT.md', 'README.md', 'THIRD-PARTY-NOTICES.html']:
                    (root / name).write_text(name)
                for directory in ['docs', 'legal', 'assets/branding', 'packaging/linux', 'sysroot/share/doc/rust/licenses']:
                    (root / directory).mkdir(parents=True)
                    (root / directory / 'notice.txt').write_text('notice')
                (root / 'sysroot/share/doc/rust/COPYRIGHT-library.html').write_text('runtime notices')
                for icon in ['sketchotter.png', 'sketchotter.icns']:
                    (root / 'assets/branding' / icon).write_bytes(b'icon')
                binary = 'sketchotter.exe' if 'windows' in target else 'sketchotter'
                path = root / 'target' / target / 'debug' / binary
                path.parent.mkdir(parents=True)
                path.write_bytes(b'fixture executable')
                path.chmod(0o755)

                def output(command, **kwargs):
                    if command == ['rustc', '--print', 'sysroot']:
                        return str(root / 'sysroot') + '\n'
                    if command == ['rustc', '--version']:
                        return 'rustc fixture\n'
                    if command == ['git', 'rev-parse', 'HEAD']:
                        return 'a' * 40 + '\n'
                    self.fail(f'Unexpected command: {command}')

                with patch.object(packager, 'ROOT', root), patch('sys.argv', ['packager', '--target', target, '--profile', 'debug']), patch.object(packager.subprocess, 'check_output', side_effect=output), patch.object(packager.subprocess, 'run') as signing:
                    packager.main()
                name = f'sketchotter-v{version}-{suffix}'
                extension = '.tar.gz' if 'linux' in target else '.zip'
                archive = root / 'dist' / (name + extension)
                self.assertTrue(archive.is_file())
                self.assertTrue(Path(str(archive) + '.sha256').read_text().endswith(archive.name + '\n'))
                stage = root / 'dist' / name
                metadata = json.loads((stage / 'BUILD-INFO.json').read_text())
                self.assertEqual(metadata['commit'], 'a' * 40)
                self.assertEqual(metadata['target'], target)
                self.assertEqual(metadata['profile'], 'debug')
                self.assertEqual(metadata['source'], f'https://example.com/project/releases/tag/v{version}')
                self.assertEqual(metadata['source_asset'], f'sketchotter-v{version}-source.tar.gz')
                if extension == '.zip':
                    with zipfile.ZipFile(archive) as zipped:
                        members = zipped.namelist()
                        executable = next(i for i in zipped.infolist() if i.filename.endswith('/' + binary))
                        self.assertTrue(executable.external_attr >> 16 & 0o111)
                else:
                    with tarfile.open(archive) as zipped:
                        members = zipped.getnames()
                        self.assertTrue(zipped.getmember(name + '/' + binary).mode & 0o111)
                        self.assertEqual(zipped.getmember(name + '/clay-form').linkname, binary)
                        self.assertIn(name + '/packaging/linux/sketchotter.png', members)
                for required in ['LICENSE', 'COPYRIGHT.md', 'THIRD-PARTY-NOTICES.html', 'legal/rust/COPYRIGHT-library.html', 'BUILD-INFO.json']:
                    self.assertIn(name + '/' + required, members)
                if 'apple' in target:
                    self.assertIn(name + '/SketchOtter.app/Contents/Resources/legal/rust/COPYRIGHT-library.html', members)
                    self.assertIn(name + '/SketchOtter.app/Contents/Resources/LICENSE', members)
                    plist = plistlib.loads((stage / 'SketchOtter.app/Contents/Info.plist').read_bytes())
                    self.assertEqual(plist['CFBundleExecutable'], binary)
                    self.assertIn(name + '/SketchOtter.app/Contents/Resources/' + plist['CFBundleIconFile'], members)
                    self.assertEqual(plist['CFBundleShortVersionString'], version.split('-')[0])
                    self.assertGreater(int(plist['CFBundleVersion']), 0)
                    signing.assert_called_once()
                else:
                    signing.assert_not_called()


if __name__ == '__main__':
    unittest.main()
