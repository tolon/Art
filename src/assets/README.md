# Assets

## `logo.*`

The project's mark, shown in Settings → About beside the program's name.

Drop it here as `logo.png` (or `.svg`, `.jpg`, `.jpeg`, `.webp`) and it appears
— the panel finds it with `import.meta.glob`, so there is no code to change and
nothing to register. Remove it and the panel simply has no logo; the build does
not care either way.

An SVG would be the better file if one exists: the panel draws it at 56 px, and
a large PNG is carried into the installer at full size for no gain.

Nothing here is Amiga content. ART ships no OS, no ROM and no distribution
files, ever.
