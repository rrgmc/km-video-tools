//! Draws this program's application icon.
//!
//! ```sh
//! cargo run -p km-video-downloader --example icon
//! ```
//!
//! Writes `icon/` at the repository root: the PNGs the page and a macOS bundle read, and the `.ico`
//! `build.rs` puts inside the Windows executable.
//!
//! # Why there is a drawing here at all
//!
//! Because these programs are run beside karaokemachine's on one desktop, and two taskbar buttons
//! wearing the same icon are not tellable apart. The icon has to belong to that family and not be a
//! copy of any of it.
//!
//! So this is the **same drawing under a fifth palette**: angular bands of color filling the tile, a
//! near-black plate over them, and `KM` on the plate — the K in near-white, the M in the hue that
//! names the program. karaokemachine leads with amber for the machine, blue for the package
//! builder, green for the offline remote and magenta for km-admin; this leads with a **cyan** none
//! of them uses, which is the only thing that has to differ for the icons to be tellable apart at
//! 16 pixels.
//!
//! # Own code rather than a shared one
//!
//! The original renderer is `crates/playback/km-display/examples/icon.rs` over there, and it reads
//! its colors out of `km_display::theme::Theme` and its types out of SDL. Neither is reachable from
//! a repository whose whole point is that it depends on nothing of karaokemachine's — so the
//! geometry below is written out again and the four colors it borrows are written down as literals.
//! **That is a copy and it can drift**, which is accepted for the same reason the packaging profile
//! is copied: the alternative was a dependency far heavier than the thing being borrowed. If the two
//! ever disagree, nothing breaks — they are different programs' icons and are *supposed* to differ.
//!
//! Everything is signed distance functions over a unit square, so one drawing serves a 16-pixel
//! favicon and a 512-pixel bundle icon rather than two that can drift apart. Rendering is
//! deterministic: re-running produces byte-identical files, so it is not a diff.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::{ImageFormat, Rgba, RgbaImage};

/// The sizes written as PNG.
///
/// 16 through 256 are what Windows and the web want; 512 is for a macOS bundle, whose `.icns` is
/// assembled from these by `tools/dist/`.
const SIZES: [u32; 7] = [16, 32, 48, 64, 128, 256, 512];

/// The sizes packed into the `.ico`.
///
/// Windows picks per context — 16 in a title bar, 32 in the taskbar, 48 in a large-icon folder view,
/// 256 for the tile. Anything larger is weight in an executable for nothing to read.
const ICO_SIZES: [u32; 4] = [16, 32, 48, 256];

// -- the palette ----------------------------------------------------------------------------------
//
// Four of these five are karaokemachine's own theme values, written down rather than imported. The
// fifth is this program's, and is the only one that makes the icon its own.

/// The deep violet the bands sit on. `Theme::icon_ground`.
const GROUND: [f32; 3] = rgb(0x2A, 0x0E, 0x42);
/// The magenta the middle band leads with. `Theme::icon_glow`.
const GLOW: [f32; 3] = rgb(0xC4, 0x3A, 0x8E);
/// The plate. `Theme::background` — a television ground and a plate are both dark objects.
const PLATE_COLOR: [f32; 3] = rgb(0x0A, 0x0C, 0x14);
/// The K. `Theme::lyric_pending`, the near-white a word is waiting in.
const LETTER: [f32; 3] = rgb(0xEC, 0xEF, 0xF4);

/// **The fifth lead, and this program's own.**
///
/// A **vermilion**, and it is picked by hue distance rather than by taste. The four in
/// karaokemachine sit at 45° (the machine's amber `FFC107`), 148° (the remote's green `57E79A`),
/// 196° (the package builder's blue `5FD3FF`) and 324° (km-admin's magenta `C43A8E`). This is 11°,
/// which is 34° from its nearest neighbour.
///
/// **The first attempt was a cyan at 180°, and it was wrong**: that is 16° from the package
/// builder's blue and 32° from the remote's green — two icons that would have been hard to tell
/// apart on one taskbar, which is the single thing a per-program palette exists to prevent.
///
/// The gaps between the four leave two real openings: about 260° and about 5°. **260° is a violet
/// and is refused**, because the tile's own ground is the deep violet `GROUND` and its middle band
/// is `GLOW` — a violet lead would make the whole icon one hue with nothing to catch at 16 pixels.
/// A warm lead gives the tile a magenta-to-vermilion run instead, which is the contrast the bands
/// are there for.
///
/// Bright enough that the M clears a 4.5:1 contrast floor against the plate, which is a constraint
/// rather than a preference: measured against `PLATE_COLOR` it is about 6.3:1.
const LEAD: [f32; 3] = rgb(0xFF, 0x5C, 0x38);

// -- the tile -------------------------------------------------------------------------------------

/// How much of an ordinary tile the plate takes up.
const PLATE: f32 = 0.62;

/// ...and how much of a small one.
///
/// At 16 pixels the ordinary fraction leaves the letters ten pixels to live in, which is not enough
/// for two of them. The plate grows and the bands become a frame rather than a field.
const PLATE_SMALL: f32 = 0.80;

/// The plate's corner radius, in plate units.
const PLATE_RADIUS: f32 = 0.16;

/// Below this the plate takes [`PLATE_SMALL`].
const SMALL: u32 = 48;

/// Where the first band gives way to the second, and the second to the third, along the
/// top-left-to-bottom-right diagonal.
///
/// Unequal on purpose: the lead hue gets the largest share because it is the only thing that says
/// which program this is, and most of the tile is about to be covered by the plate — so what is
/// being divided is the frame around it rather than the square.
const BAND_ONE: f32 = 0.34;
const BAND_TWO: f32 = 0.58;

/// How much darker the corners are than the middle.
const VIGNETTE: f32 = 0.12;

/// The monogram, as one coordinate system.
mod monogram {
    /// How much of its own drawing the mark is set at, about the middle of the plate.
    ///
    /// **One number rather than a nudge per constant.** Everything below is one coordinate system,
    /// so moving the cap height without the arms, the waist and the vertex would draw a different
    /// pair of letters rather than smaller ones.
    pub const SCALE: f32 = 0.85;

    /// The band every terminal is cut to. Symmetric about the middle of the plate.
    pub const CAP_TOP: f32 = 0.295;
    pub const BASELINE: f32 = 0.705;

    /// Half the weight of a stroke. A quarter of the cap height — heavier than a text face would
    /// ever be set, and about right for a mark: a stroke has to survive being a pixel and a half
    /// wide at 32 pixels, and the counters have to survive being about as thin.
    pub const WEIGHT: f32 = 0.052;

    /// How far a stroke runs past the band before the band cuts it. Only has to exceed
    /// [`WEIGHT`], so the round cap it hides clears the edge.
    pub const OVERRUN: f32 = 0.070;

    /// **K**: an upright, and two arms meeting it at the waist.
    pub const K_STEM: f32 = 0.152;
    pub const K_WAIST: (f32, f32) = (0.152, 0.500);
    /// Where each arm crosses the band, extended outward from the waist — so moving the band moves
    /// the letter rather than breaking it.
    pub const K_ARM_TIP: (f32, f32) = (0.343, CAP_TOP);
    pub const K_LEG_TIP: (f32, f32) = (0.360, BASELINE);

    /// **M**: two uprights, and a V between them. Each diagonal starts at the top of its own
    /// upright, so the joins at the cap line need no help; the round caps meeting at the vertex are
    /// what closes the V at the bottom.
    pub const M_LEFT: f32 = 0.560;
    pub const M_RIGHT: f32 = 0.848;
    pub const M_VERTEX: (f32, f32) = (0.704, 0.612);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = repository_root().join("icon");
    std::fs::create_dir_all(&dir)?;

    for size in SIZES {
        let image = render(size);
        let path = dir.join(format!("km-video-downloader-{size}.png"));
        write_if_changed(&path, &png(&image)?)?;
    }

    write_if_changed(&dir.join("km-video-downloader.ico"), &ico()?)?;
    write_if_changed(&dir.join("km-video-downloader.icns"), &icns()?)?;

    println!("wrote {}", dir.display());
    Ok(())
}

/// The repository root, from this file's own location at build time.
///
/// `CARGO_MANIFEST_DIR` is the crate, and the root is two above it. Asked of cargo rather than of
/// the working directory, so the example writes the same place wherever it is run from.
fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// One icon at one size.
fn render(size: u32) -> RgbaImage {
    let plate_share = if size < SMALL { PLATE_SMALL } else { PLATE };
    let plate_origin = (1.0 - plate_share) / 2.0;

    // One pixel, in canvas units. Every antialiased edge is measured against this, which is what
    // lets the same drawing serve 16 pixels and 512.
    let pixel = 1.0 / size as f32;

    let mut image = RgbaImage::new(size, size);
    for y in 0..size {
        for x in 0..size {
            // The center of the pixel, not its corner: a shape passing exactly through the middle
            // should cover half of it.
            let p = (
                (x as f32 + 0.5) / size as f32,
                (y as f32 + 0.5) / size as f32,
            );

            // The tile: a rounded square filling the canvas, holding the bands.
            let tile = rounded_rect(p, (0.0, 0.0), (1.0, 1.0), PLATE_RADIUS * plate_share);
            let (mut color, mut alpha) = (bands(p, pixel, y), coverage(tile, pixel));

            // The plate over them.
            let plate_box = (
                (plate_origin, plate_origin),
                (plate_origin + plate_share, plate_origin + plate_share),
            );
            let plate = rounded_rect(p, plate_box.0, plate_box.1, PLATE_RADIUS * plate_share);
            (color, alpha) = over(PLATE_COLOR, coverage(plate, pixel), color, alpha);

            // ...and the letters on the plate, measured in the plate's own coordinates so the mark
            // cannot drift out of the square it is cropped by.
            let q = (
                (p.0 - plate_origin) / plate_share,
                (p.1 - plate_origin) / plate_share,
            );
            let (k, m) = monogram_distance(q);
            let letter_pixel = pixel / plate_share;
            (color, alpha) = over(LETTER, coverage(k, letter_pixel), color, alpha);
            (color, alpha) = over(LEAD, coverage(m, letter_pixel), color, alpha);

            image.put_pixel(
                x,
                y,
                Rgba([byte(color[0]), byte(color[1]), byte(color[2]), byte(alpha)]),
            );
        }
    }
    image
}

/// The ground: three straight-edged bands running bottom-left to top-right, each with a gradient.
///
/// **Angular rather than a radial wash.** A radial gradient has no edges in it, so a tile made of
/// one is a surface and nothing else; bands give the eye something to catch at 16 pixels, which is
/// the size at which an icon is either recognized or not. The cost is that a seam is a hard edge
/// and needs antialiasing, which is one `coverage` call — the same machinery the letters use.
fn bands(p: (f32, f32), pixel: f32, row: u32) -> [f32; 3] {
    // Across the bands, 0 at the top-left corner and 1 at the bottom-right...
    let across = (p.0 + p.1) / 2.0;
    // ...and along them, 0 at the bottom-left and 1 at the top-right. Every band's gradient runs on
    // this, so the light in the tile has one direction rather than three.
    let along = (p.0 - p.1 + 1.0) / 2.0;

    // Perpendicular distance to each seam. The factor turns a difference in `across` — measured
    // along the diagonal — back into a real distance, which is what `pixel` is in.
    let seam = |at: f32| (across - at) * 2.0 / std::f32::consts::SQRT_2;
    let band = |(deep, bright): ([f32; 3], [f32; 3])| mix(bright, deep, along.clamp(0.0, 1.0));

    // The dark band is a deep purple lifted toward the magenta rather than `GROUND` itself: straight
    // it is about 1.2:1 against the plate, which is not a dark corner but a missing one — the
    // plate's top-left edge disappears into it.
    let violet = (mix(GROUND, GLOW, 0.44), mix(GROUND, GLOW, 0.82));
    let magenta = (mix(GLOW, GROUND, 0.22), toward_white(GLOW, 0.08));
    let lead = (scaled(LEAD, 0.76), toward_white(LEAD, 0.12));

    let mut color = band(violet);
    color = mix(color, band(magenta), 1.0 - coverage(seam(BAND_ONE), pixel));
    color = mix(color, band(lead), 1.0 - coverage(seam(BAND_TWO), pixel));

    let from_center = ((p.0 - 0.5).powi(2) + (p.1 - 0.5).powi(2)).sqrt() * 2.0;
    let vignette = 1.0 - VIGNETTE * from_center.clamp(0.0, 1.4).powi(2);

    // A per-row dither: eight-bit color cannot hold a gradient this shallow without faint contour
    // rings, and offsetting whole rows breaks them up without destroying the horizontal runs PNG
    // compresses.
    let dither = if row.is_multiple_of(2) {
        0.0
    } else {
        1.0 / 255.0
    };
    color.map(|value| value * vignette + dither)
}

/// Signed distance to the **K** and to the **M** separately, in plate units. Negative inside.
///
/// Two distances rather than one because the two letters are not the same color: the site's wordmark
/// sets `Karaoke` in the text color and `Machine` in the lead, and this is that wordmark with
/// everything but the initials taken away.
fn monogram_distance(q: (f32, f32)) -> (f32, f32) {
    use monogram::{
        BASELINE, CAP_TOP, K_ARM_TIP, K_LEG_TIP, K_STEM, K_WAIST, M_LEFT, M_RIGHT, M_VERTEX,
        OVERRUN, SCALE, WEIGHT,
    };

    // The mark is set at `SCALE` about the middle of the plate, and the cheapest place to do that is
    // here: the point is moved into the letters' own coordinates and the distance scaled to undo it.
    let q = ((q.0 - 0.5) / SCALE + 0.5, (q.1 - 0.5) / SCALE + 0.5);

    let upright = |x: f32| capsule(q, (x, CAP_TOP - OVERRUN), (x, BASELINE + OVERRUN), WEIGHT);
    let arm = |tip: (f32, f32)| capsule(q, K_WAIST, extended(tip, K_WAIST, OVERRUN), WEIGHT);
    let diagonal = |top: f32| {
        capsule(
            q,
            extended((top, CAP_TOP), M_VERTEX, OVERRUN),
            M_VERTEX,
            WEIGHT,
        )
    };

    let k = upright(K_STEM).min(arm(K_ARM_TIP)).min(arm(K_LEG_TIP));
    let m = upright(M_LEFT)
        .min(upright(M_RIGHT))
        .min(diagonal(M_LEFT))
        .min(diagonal(M_RIGHT));

    // The band, as an intersection. Wide enough that only its top and bottom edges can ever bite,
    // which is the point: it squares off terminals and never touches a letter's sides.
    let band = rounded_rect(q, (-1.0, CAP_TOP), (2.0, BASELINE), 0.0);
    (k.max(band) * SCALE, m.max(band) * SCALE)
}

// -- the primitives -------------------------------------------------------------------------------

/// `from`, pushed `amount` further away from `to` along the line through both.
fn extended(from: (f32, f32), to: (f32, f32), amount: f32) -> (f32, f32) {
    let (dx, dy) = (from.0 - to.0, from.1 - to.1);
    let length = dx.hypot(dy);
    (from.0 + dx / length * amount, from.1 + dy / length * amount)
}

/// Signed distance to a rounded rectangle, given by two opposite corners.
fn rounded_rect(p: (f32, f32), min: (f32, f32), max: (f32, f32), radius: f32) -> f32 {
    let center = ((min.0 + max.0) / 2.0, (min.1 + max.1) / 2.0);
    let half = (
        (max.0 - min.0) / 2.0 - radius,
        (max.1 - min.1) / 2.0 - radius,
    );
    let d = (
        (p.0 - center.0).abs() - half.0,
        (p.1 - center.1).abs() - half.1,
    );
    let outside = (d.0.max(0.0).powi(2) + d.1.max(0.0).powi(2)).sqrt();
    let inside = d.0.max(d.1).min(0.0);
    outside + inside - radius
}

/// Signed distance to a capsule: the segment `a`–`b`, thickened by `radius`.
///
/// Every stroke of the monogram is one of these, diagonals included — which is why there is no
/// rotation helper here: a rotated stroke is a segment whose ends are somewhere else.
fn capsule(p: (f32, f32), a: (f32, f32), b: (f32, f32), radius: f32) -> f32 {
    let (px, py) = (p.0 - a.0, p.1 - a.1);
    let (bx, by) = (b.0 - a.0, b.1 - a.1);
    let along = ((px * bx + py * by) / (bx * bx + by * by)).clamp(0.0, 1.0);
    (px - bx * along).hypot(py - by * along) - radius
}

/// How much of a pixel a shape covers, from the signed distance at its center.
///
/// The whole antialiasing story, and exact for a straight edge: an edge half a pixel away covers
/// none of it, one through the center covers half.
fn coverage(distance: f32, pixel: f32) -> f32 {
    (0.5 - distance / pixel).clamp(0.0, 1.0)
}

/// Source over destination, premultiplied properly so an edge over an edge does not darken.
fn over(src: [f32; 3], src_alpha: f32, dst: [f32; 3], dst_alpha: f32) -> ([f32; 3], f32) {
    let alpha = src_alpha + dst_alpha * (1.0 - src_alpha);
    if alpha <= 0.0 {
        return ([0.0; 3], 0.0);
    }
    let mut out = [0.0; 3];
    for (index, value) in out.iter_mut().enumerate() {
        *value = (src[index] * src_alpha + dst[index] * dst_alpha * (1.0 - src_alpha)) / alpha;
    }
    (out, alpha)
}

const fn rgb(r: u8, g: u8, b: u8) -> [f32; 3] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

fn toward_white(color: [f32; 3], amount: f32) -> [f32; 3] {
    color.map(|value| value + (1.0 - value) * amount)
}

fn scaled(color: [f32; 3], factor: f32) -> [f32; 3] {
    color.map(|value| value * factor)
}

fn mix(from: [f32; 3], to: [f32; 3], t: f32) -> [f32; 3] {
    let mut out = [0.0; 3];
    for (index, value) in out.iter_mut().enumerate() {
        *value = from[index] + (to[index] - from[index]) * t;
    }
    out
}

fn byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

// -- the containers -------------------------------------------------------------------------------

/// Encodes an image as PNG in memory.
fn png(image: &RgbaImage) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    image.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)?;
    Ok(bytes)
}

/// The Windows container: several sizes in one file, each a PNG.
fn ico() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut directory = ico_dir();
    let mut images = Vec::new();
    for size in ICO_SIZES {
        images.push(png(&render(size))?);
    }

    // The 6-byte header, then a 16-byte entry per image, then the images.
    let mut offset = 6 + 16 * ICO_SIZES.len();
    for (index, size) in ICO_SIZES.iter().enumerate() {
        let bytes = &images[index];
        // 0 means 256 in this format, which is why the field is a byte at all.
        let dimension = u8::try_from(*size).unwrap_or(0);
        directory.extend_from_slice(&[dimension, dimension, 0, 0]);
        directory.extend_from_slice(&1u16.to_le_bytes()); // color planes
        directory.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        directory.extend_from_slice(&u32::try_from(bytes.len())?.to_le_bytes());
        directory.extend_from_slice(&u32::try_from(offset)?.to_le_bytes());
        offset += bytes.len();
    }
    for bytes in images {
        directory.extend_from_slice(&bytes);
    }
    Ok(directory)
}

/// The macOS container: the same drawing again, each size a PNG under a four-character type.
///
/// **Written here rather than by `iconutil`**, which is a macOS-only tool — and a `.app` staged from
/// a Windows or Linux machine would otherwise have no icon at all. The format is simple enough that
/// not needing the tool is worth the twenty lines: a magic word, the total length, then one typed
/// chunk per image.
///
/// The `ic07`..`ic09` types are the retina-era PNG ones; the older `is32`/`il32` bitmap-and-mask
/// pairs are not written, so this is read by 10.7 and later — which is everything.
fn icns() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    /// Each size and the four-character type macOS files it under.
    const CHUNKS: [(u32, &[u8; 4]); 3] = [(128, b"ic07"), (256, b"ic08"), (512, b"ic09")];

    let mut body = Vec::new();
    for (size, kind) in CHUNKS {
        let png = png(&render(size))?;
        body.extend_from_slice(kind);
        // The length includes the eight bytes of the header itself.
        body.extend_from_slice(&u32::try_from(png.len() + 8)?.to_be_bytes());
        body.extend_from_slice(&png);
    }

    let mut out = Vec::with_capacity(body.len() + 8);
    out.extend_from_slice(b"icns");
    out.extend_from_slice(&u32::try_from(body.len() + 8)?.to_be_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// The `.ico` header: reserved, type 1 (icon), and how many images follow.
fn ico_dir() -> Vec<u8> {
    let mut header = Vec::new();
    header.extend_from_slice(&0u16.to_le_bytes());
    header.extend_from_slice(&1u16.to_le_bytes());
    header.extend_from_slice(&u16::try_from(ICO_SIZES.len()).unwrap_or(0).to_le_bytes());
    header
}

/// Writes a file only when its contents would change.
///
/// Rendering is deterministic, so re-running the example is not a diff — and `build.rs` watches the
/// `.ico`, which would otherwise relink the executable every time this was run.
fn write_if_changed(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if std::fs::read(path).is_ok_and(|existing| existing == bytes) {
        return Ok(());
    }
    std::fs::write(path, bytes)
}
