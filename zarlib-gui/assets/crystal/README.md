# Crystal icons

These are original project icons, generated from the same vector designs as
the Flat theme with gradients, bevels and a glossy finish. They are licensed
under MIT OR Apache-2.0, like the rest of the project. They do not use the
former KDE Crystal artwork.

To regenerate them and the matching zarmount button icons, run from the
repository root with Playwright and its Chromium browser installed:

```sh
node scripts/generate-flat-icons.cjs --crystal
```

Icon names may be supplied to regenerate individual icons. The generator uses
the Flat PNG files only for their names and dimensions; the artwork is defined
in the generator. Generated icons are at least 48 by 48 pixels; larger icons
keep their size.
