# The substrate

## What

A binary file (magic `BRYSUBST`, format version 4) with a SHA-256 over the whole file. It holds:

- **Tables**: named tables of numbers: permutations, noise and wave data, star colours and similar constants. Each is a fixed shape (count and element type: 32-bit or 64-bit float, integers, bytes) or a list of labels.
- **Kept files**: names, hashes and lengths of files stored in the folder `SUBSTRATE.files` beside it: a moon picture, a star catalogue, leaves for trees, preset libraries. Each has a role byte.
- A source record for each entry: which install it came from.

## Why it exists

The renderer needs constants and data that are not computed by any formula we can write. We need those exact values to get exact pictures. They cannot be published with the source, so they live in a separate file made on the user's machine from their own install. The code carries no such data.

This also keeps the code honest: no table has a default or a zero fill. A render that needs a table the file lacks fails with the table's name.

## How it is read

`substrate.rs` reads the whole file into memory, validates everything (magic, version, sizes, every entry's hash, the file hash), and only then returns a `Substrate`. A refused file gives no value, so no half-valid entry can leak out. Typed accessors return fixed-size arrays so a consumer cannot read past a table. Old formats (1 to 3) are refused with advice to make the file again.

Finding the file: the command-line option, then the environment variable `BRYCE_SUBSTRATE`, then `substrate.bin` in a folder named `stecyk3d` in the user's application data (`APPDATA` on Windows, `~/Library/Application Support` on macOS, `$XDG_DATA_HOME` or `~/.local/share` elsewhere). The add-on follows the same order, with its preferences first, and also reads the default folder of the first release when the new one has no file.

## How it is made

`extract.rs` (the `stecyk3d-extract` program) recognises each given folder as a supported install, collects entries, writes the file and the `.files` folder, and writes nothing if any folder is refused. The add-on's button runs the same program.
