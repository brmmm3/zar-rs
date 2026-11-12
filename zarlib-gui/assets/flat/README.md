# Flat icons

This directory contains a flat, vector-based alternative to the icons in
[`../crystal/`](../crystal/). Each icon has the same relative path, PNG filename,
and dimensions as its counterpart. Editable SVG sources are included alongside
the PNG files. These are new illustrations of the same actions and file types,
not recolored copies of the original artwork.

Select this theme in the application's Settings dialog.
To regenerate the files from the repository root, with the existing Playwright
package and its Chromium browser installed, run:

```sh
node scripts/generate-flat-icons.cjs
```
