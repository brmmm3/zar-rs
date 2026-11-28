# Flat icons

This directory contains a flat, vector-based alternative to the icons in
[`../crystal/`](../crystal/). Each icon has the same relative path, PNG filename,
and dimensions as its counterpart. Both themes use original project artwork,
licensed under MIT OR Apache-2.0. The vector designs are defined in the
generator.
Generated icons are at least 48 by 48 pixels; larger icons keep their size.

Select this theme in the application's Settings dialog.
To regenerate the files from the repository root, with the existing Playwright
package and its Chromium browser installed, run:

```sh
node scripts/generate-flat-icons.cjs
```

Pass icon names to regenerate only those icons. Set
`PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH` to use an installed Chromium-compatible
browser instead of Playwright's downloaded browser.
