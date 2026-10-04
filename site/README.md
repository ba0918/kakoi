# Landing page

Python 3.11+ and Node.js 20.19+ or 22.12+ are required for the local and Pages checks.

```sh
python3 -m pip install -r site/requirements.txt
npm ci --prefix site --ignore-scripts
site/node_modules/.bin/oxfmt --check site
python3 site/test_build.py
python3 site/build.py --out _site
python3 site/check.py --out _site
python3 -m http.server --directory _site
```

`template.html` contains the structure, `messages.json` owns both languages,
`style.css` owns the layout, and `app.js` applies translations. Messages may contain
reviewed HTML; the template marks that editorial markup as safe. Do not put untrusted
input in this dictionary. Jinja2 fails on missing template values, and the build
requires matching English and Japanese keys. The English HTML works without JavaScript.
The version still comes from `Cargo.toml`; the source commit comes from `GITHUB_SHA`
in CI or the local Git HEAD.

`language-preference.js` follows the same contract as the other ba0918 sites:
valid `ba0918-language` (`en`/`ja`), then this page's legacy `kakoi-lp-language`, then
English. Reading never writes or promotes a legacy value. Only an explicit language
button click saves the shared value. Storage denial still permits in-page switching.
Other pages inherit the choice on navigation or reload; open tabs need not synchronize.

`.oxfmtrc.json` sets the formatting of the CSS, JavaScript, JSON and Markdown here; run
`site/node_modules/.bin/oxfmt site` to apply it. The version is fixed in
`package.json` and `package-lock.json`. `template.html` is excluded: to keep inline
whitespace, the formatter wraps inline markup as `>…</a\n>`, which is harder to read.
Keep its indentation by hand, and keep text next to inline elements and `<pre>` contents
on one line, since a line break there adds visible space.

`check.py` checks JavaScript, the language contract, translation references, local assets
and fragment links. It does not fetch external links. For layout changes, compare English
and Japanese at 1440, 390 and 320 pixels; also check a primary CTA and navigation from
the portfolio under the same origin.
