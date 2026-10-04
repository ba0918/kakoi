#!/usr/bin/env python3
"""Build the landing page: python3 site/build.py --out _site."""

import argparse
import json
import os
import shutil
import subprocess
import tomllib
from pathlib import Path

from jinja2 import Environment, StrictUndefined

SITE = Path(__file__).resolve().parent
ROOT = SITE.parent


def render(template, messages, **values):
    if set(messages) != {'en', 'ja'} or messages['en'].keys() != messages['ja'].keys():
        raise ValueError('English and Japanese translation keys must match')
    if any(not isinstance(value, str) for copy in messages.values() for value in copy.values()):
        raise ValueError('Translations must be strings')
    engine = Environment(undefined=StrictUndefined, autoescape=True, keep_trailing_newline=True)
    page = engine.from_string(template)
    # Both locales must render even though the static, no-JS page is English.
    pages = {lang: page.render(copy=copy, **values) for lang, copy in messages.items()}
    return pages['en']


def build(out, commit):
    messages = json.loads((SITE / 'messages.json').read_text(encoding='utf-8'))
    cargo = tomllib.loads((ROOT / 'Cargo.toml').read_text(encoding='utf-8'))
    page = render((SITE / 'template.html').read_text(encoding='utf-8'), messages,
                  version=cargo['workspace']['package']['version'], commit=commit)
    out.mkdir(parents=True, exist_ok=True)
    (out / 'index.html').write_text(page, encoding='utf-8')
    (out / 'messages.js').write_text('const messages = ' + json.dumps(messages, ensure_ascii=False) + ';\n', encoding='utf-8')
    for asset in ('style.css', 'app.js', 'language-preference.js'):
        shutil.copyfile(SITE / asset, out / asset)
    print(f'Built {out / "index.html"}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, default=ROOT / '_site')
    args = parser.parse_args()
    commit = os.environ.get('GITHUB_SHA') or subprocess.check_output(
        ['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    build(args.out, commit)


if __name__ == '__main__':
    main()
