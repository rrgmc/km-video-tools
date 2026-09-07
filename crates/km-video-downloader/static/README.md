# Static assets

**Everything here is compiled into the executable** with `include_str!` and served by an explicit
route in `src/server.rs`. There is no `ServeDir`, no npm, no bundler and no minifier — the pipeline
is `include_str!`.

That is not thrift. This program is a single file somebody copies onto a desktop; a `static/`
directory that had to arrive beside it is 55 KB of ways to be half-installed.

**Nothing is fetched from a CDN, ever.** A page that needed unpkg to render its own controls would
break with nothing on screen to say why — and this is a program somebody runs to *get* things off
the network, so it should not need the network to draw itself.

| File | What it is |
|---|---|
| `htmx.min.js` | htmx 2.0.4, unmodified. The same copy the karaoke app's three pages vendor. |
| `htmx-LICENSE.txt` | htmx's license, 0BSD, served at `/static/htmx-LICENSE.txt` because a vendored dependency's terms travel with it. |
| `style.css` | Hand-written, no framework. Light, and pinned — see the note at the top of it. |
| `ui.js` | This program's own, and small on purpose. |
| `icon.svg` | The tab's mark. An SVG so that every file here is a text file. |

## Four things htmx will catch you with

Three cost time in the programs this one is modelled on. The fourth is this program's own, and it
hid the first one.

- **htmx does not swap a non-2xx response.** A handler that returns 400 with a perfectly good
  explanation puts nothing on the page at all, so a failure looks like a dead button. `ui.js` listens
  for `htmx:responseError` and `htmx:sendError` and is most of why it exists.
- **A listener on `document.body` does not survive a swap of the body.** Setting the output folder
  answers with the whole page and swaps it in with `hx-target="body" hx-swap="outerHTML"` — which
  replaces the body element and takes every listener attached to it. Bound to the body, the error
  handling above worked exactly until somebody chose a folder, after which every refusal was silent
  again. Found by watching a perfectly good "that is not a browser yt-dlp knows" go nowhere. htmx's
  events bubble to `document`, which nothing replaces, so that is where they are bound.
- **A Windows path in `hx-vals` must go through `|json|safe`**, never `"{{ path }}"`. askama's HTML
  escaper leaves backslashes alone, so `D:\tunes\karaoke` becomes invalid JSON — `\t` is a tab and
  `\k` is nothing — `JSON.parse` throws, and htmx sends an **empty body**. Every click is then
  answered "No folder given", about a folder that was picked perfectly well. In a query string the
  filter is `|urlencode` instead.
- **`hx-post` on a file form needs `hx-encoding="multipart/form-data"`.** htmx does not read the
  form's own `enctype`, so without it the file is sent URL-encoded as its *filename* and the program
  reads a list of links that is one line long and is a file name. The Fetch form carries both:
  `hx-encoding` for htmx, and `enctype` for the case where the script did not load at all.
