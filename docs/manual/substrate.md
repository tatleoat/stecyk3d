# Making the substrate

## What it is

The substrate is one file (plus a folder of small files beside it) that holds fixed data a render needs: lookup tables used by procedural textures and noise, a few pictures such as the moon's, a star catalogue, the Tree Lab's leaves, and the preset libraries.

It is made from **your own copy** of Bryce. Stecyk3D does not ship it: that data belongs to Bryce, and nothing in this package contains any of it. The extractor reads your install and writes the substrate on your machine. You do it once.

Without a substrate, a render stops and says what is missing. The program never guesses a value.

## Make it from Blender (the easy way)

1. Open *Edit > Preferences > Add-ons > Stecyk3D*.
2. Fill in **Bryce 3.0 folder**, **Bryce 3.1 folder** and/or **Bryce 5.0 folder** with the install folders you have.
3. Press **Make substrate**.

On success an INFO message says where the substrate was written and, per version, what was recognised and how many kept files were taken. On failure nothing is written and the message says exactly why (for example "no Bryce program file found", or "this is Bryce 3D 3.1, not Bryce 3.0: put it in the Bryce 3.1 folder").

The substrate is written to the file named by the `BRYCE_SUBSTRATE` environment variable if that is set, else to the default place below, and is used from then on. There is no field to fill in afterwards.

Name every install you have. Each adds what only it holds: a Bryce 5.0 sky that uses the star catalogue needs a Bryce 5 install, and a render that needs something your substrate lacks stops and names it. If that happens, add the install and press **Make substrate** again.

## Make it with the command line

```
stecyk3d-extract [--out SUBSTRATE.bin] [--bryce 3.0=FOLDER ...] [FOLDER ...]
```

Example:

```
stecyk3d-extract --out ~/stecyk/substrate.bin --bryce 3.0="/path/to/Bryce 3D 3.0" --bryce 5.0="/path/to/Corel Bryce 5.0"
```

- `--bryce V=FOLDER` (V is 3.0, 3.1 or 5.0) also refuses a folder that holds another version than V. A bare `FOLDER` takes whatever version it holds.
- A folder may be the install itself or hold it up to two levels below it. `Data`/`data` and every other name are matched without regard to letter case.
- The extractor writes `SUBSTRATE.bin` and a folder `SUBSTRATE.files` beside it. Keep them together.
- It writes nothing if any folder is refused. Refusals are printed on standard error and the exit status is 1. On success it prints what it recognised and what it took.
- Without `--out`, it writes `substrate.bin` to the default place: `stecyk3d/substrate.bin` inside your application-data folder (`%APPDATA%` on Windows, `~/Library/Application Support` on macOS, `$XDG_DATA_HOME` or `~/.local/share` on Linux).

## How programs find the substrate

In this order:

1. the `--substrate` option of `stecyk3d-render`, or the add-on's **Substrate file** field under Advanced;
2. the `BRYCE_SUBSTRATE` environment variable;
3. the default place above.

## If the file is refused

A substrate with a wrong checksum, an old format or a bad magic number is refused whole. Make it again with the current `stecyk3d-extract`.
