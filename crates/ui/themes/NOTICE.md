# Third-party notice: `catppuccin.json`

`catppuccin.json` in this directory is copied unmodified from
[`longbridge/gpui-component`](https://github.com/longbridge/gpui-component)'s
`themes/catppuccin.json`. gpui-component is licensed Apache License 2.0; a copy of that
license is at `LICENSE-APACHE` in this directory, as required by section 4(a) of the
license for anyone redistributing the file further.

The theme data itself — the four Catppuccin palettes the file encodes (Latte, Frappé,
Macchiato, Mocha) — originates from the [Catppuccin project](
https://github.com/catppuccin/catppuccin), MIT licensed:

```
MIT License

Copyright (c) 2021 Catppuccin

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

`catppuccin.json`'s own `name`, `author` and `url` fields carry this same attribution and
must not be edited out if the file is ever re-vendored from a newer gpui-component release.

Only the Frappé flavour in this file is applied by gitr, for dark mode. Latte, Macchiato and
Mocha are carried unused, since trimming them would edit a file whose value is that it is a
verbatim copy. Light mode uses `gitr-light.json` instead — see below.

# Third-party notice: `gitr-light.json`

`gitr-light.json` in this directory is gitr's own file, authored against the same schema
`catppuccin.json` uses (see `$schema` in both files). It is not a copy of anything.

The colour *values* it fills that schema with, however, are [Tailwind CSS](
https://github.com/tailwindlabs/tailwindcss)'s published `stone`, `red`, `green`, `blue`,
`indigo`, `amber`, `teal`, `purple`, `orange` and `yellow` palettes — the same palette
`remindr-v2` (this project's Rust/Dioxus sibling) styles itself with, and which this file
was directly derived from. Tailwind CSS is MIT licensed:

```
MIT License

Copyright (c) Tailwind Labs, Inc.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

(Copied from Tailwind CSS's own `LICENSE` file, MIT License, Copyright (c) Tailwind Labs,
Inc.)
