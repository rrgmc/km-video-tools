# The application icon

Everything here is generated. Do not edit these files:

```sh
cargo run -p km-video-downloader --example icon     # or: task icon
```

The renderer is
[`crates/km-video-downloader/examples/icon.rs`](../crates/km-video-downloader/examples/icon.rs), and
that is where the design lives — everything is signed distance functions over a unit square, so one
drawing serves a 16-pixel favicon and a 512-pixel bundle icon rather than two that can drift apart.
Rendering is deterministic, so re-running is not a diff.

| File | Read by |
|---|---|
| `km-video-downloader-{16,32,48,64,128,256,512}.png` | anything that wants a picture |
| `km-video-downloader.ico` | Windows, from inside the executable — see the crate's `build.rs` |
| `km-video-downloader.icns` | a macOS `.app`, staged by `tools/dist/cmd.sh` |

## Why it looks like karaokemachine's

Because it is the same drawing: **angular bands of colour filling the tile, a near-black plate over
them, and `KM` on the plate** — the K in near-white, the M in the hue that names the program. These
tools serve [karaokemachine](https://github.com/rrgmc/karaokemachine) and are run beside it, so the
mark belongs to that family.

**The lead is a cyan, and it is the only thing that had to be its own.** That repository's four are
amber for the machine, blue for the package builder, green for the offline remote and magenta for
`km-admin`. Two taskbar buttons wearing the same icon are not tellable apart, which is the whole
argument that produced four palettes there and a fifth here.

The other four colours are that project's own theme values, written down as literals rather than
imported: its renderer reads them out of `km_display::theme::Theme` and its types out of SDL, and
neither is reachable from a repository whose point is to depend on none of it. **That copy can
drift, and nothing breaks if it does** — these are different programs' icons and are supposed to
differ.

## Committed, though generated

Because `build.rs` reads the `.ico` at build time, and a fresh clone that had to run a generator
first would either fail or quietly produce an executable with Windows' default icon. It does the
second, with a `cargo:warning` saying which command to run — but only for somebody who deleted these
on purpose.
