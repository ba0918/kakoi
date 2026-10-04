import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

from build import build, render
from jinja2 import UndefinedError


class SiteBuildTests(unittest.TestCase):
    def test_missing_translation_fails_before_publication(self):
        with self.assertRaises((ValueError, KeyError)):
            render('<p>{{ copy.heading }}</p>', {'en': {'heading': 'Hello'}, 'ja': {}})

    def test_template_cannot_silently_use_an_unknown_translation(self):
        with self.assertRaises(UndefinedError):
            render('<p>{{ copy.missing }}</p>', {'en': {}, 'ja': {}})

    def test_build_produces_english_html_and_both_language_assets(self):
        with TemporaryDirectory() as directory:
            output = Path(directory)
            build(output, 'review-commit')
            page = (output / 'index.html').read_text(encoding='utf-8')
            self.assertIn('review-commit', page)
            self.assertNotIn('__KAKOI_', page)
            self.assertNotIn('{{', page)
            for asset in ('app.js', 'messages.js', 'language-preference.js', 'style.css'):
                self.assertTrue((output / asset).is_file(), asset)


if __name__ == '__main__':
    unittest.main()
