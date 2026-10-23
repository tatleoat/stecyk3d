// SPDX-License-Identifier: MIT
//! The Pictures tab of the terrain editor as 3.0 and 3.1: the rules P1 to P4 (bringing a picture to
//! another size, the canvas picture, the slots, Apply) and 3.1's Copy and Paste.
//!
//! (Outputs: P1, P2, P3, P4; Variants: 3.1's Copy, Paste and Load; Errors: a missing
//! default gradient),

use crate::terrain_pictures::{ResultMode, result_value};

/// A picture of 8-bit red, green and blue pixels, row 0 at the top.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<[u8; 3]>,
}

impl Picture {
    pub fn new(width: usize, height: usize, pixels: Vec<[u8; 3]>) -> Picture {
        assert_eq!(pixels.len(), width * height);
        Picture { width, height, pixels }
    }

    /// A picture of one colour.
    pub fn flat(width: usize, height: usize, colour: [u8; 3]) -> Picture {
        Picture { width, height, pixels: vec![colour; width * height] }
    }

    /// A picture of gray levels (the three channels equal), row after row from the top.
    pub fn gray(width: usize, height: usize, levels: &[u8]) -> Picture {
        Picture::new(width, height, levels.iter().map(|&g| [g, g, g]).collect())
    }

    fn at(&self, row: usize, column: usize) -> [u8; 3] {
        self.pixels[row * self.width + column]
    }
}

/// P1: a picture of W by H pixels brought to W' by H'. Unchanged where the sizes are equal. If
/// W' < W or H' < H and kx = cut down(W / W'), ky = cut down(H / H') are both at least 2, each new
/// pixel is the average of a block of kx by ky old pixels; in every other case it is bilinear.
/// A size of 0, or a width or height of 1 asked to grow to more (not given by the entry), gives
/// the picture's first pixel at every place.
pub fn resize(p: &Picture, new_w: usize, new_h: usize) -> Picture {
    if (p.width, p.height) == (new_w, new_h) {
        return p.clone();
    }
    let (w, h) = (p.width as i64, p.height as i64);
    let (nw, nh) = (new_w as i64, new_h as i64);
    if p.width == 0 || p.height == 0 || new_w == 0 || new_h == 0 {
        return Picture::flat(new_w, new_h, p.pixels.first().copied().unwrap_or([0, 0, 0]));
    }
    let mut out = Vec::with_capacity(new_w * new_h);
    let (kx, ky) = (w / nw, h / nh);
    if (nw < w || nh < h) && kx >= 2 && ky >= 2 {
        let sx = if nw > 1 { (w - kx) * 65536 / (nw - 1) } else { 0 };
        let sy = if nh > 1 { (h - ky) * 65536 / (nh - 1) } else { 0 };
        let m = 16_843_009 / (kx * ky);
        for i in 0..nh {
            let r0 = i * sy / 65536;
            for j in 0..nw {
                let c0 = j * sx / 65536;
                let mut sum = [0i64; 3];
                for r in r0..r0 + ky {
                    for c in c0..c0 + kx {
                        let px = p.at(r as usize, c as usize);
                        for k in 0..3 {
                            sum[k] += px[k] as i64;
                        }
                    }
                }
                out.push(std::array::from_fn(|k| (sum[k] * m / 16_777_216) as u8));
            }
        }
    } else {
        let sx = if nw > 1 { (65536 * w - 65537).div_euclid(nw - 1) } else { 0 };
        let sy = if nh > 1 { (65536 * h - 65537).div_euclid(nh - 1) } else { 0 };
        let last_c = (w - 1).max(0);
        let last_r = (h - 1).max(0);
        for i in 0..nh {
            let y0 = (i * sy).div_euclid(65536);
            let fy = (i * sy).div_euclid(256).rem_euclid(256);
            for j in 0..nw {
                let x0 = (j * sx).div_euclid(65536);
                let fx = (j * sx).div_euclid(256).rem_euclid(256);
                let get = |r: i64, c: i64| p.at(r.clamp(0, last_r) as usize, c.clamp(0, last_c) as usize);
                let (a, b) = (get(y0, x0), get(y0, x0 + 1));
                let (c, d) = (get(y0 + 1, x0), get(y0 + 1, x0 + 1));
                out.push(std::array::from_fn(|k| {
                    let t = a[k] as i64 + ((b[k] as i64 - a[k] as i64) * fx).div_euclid(256);
                    let u = c[k] as i64 + ((d[k] as i64 - c[k] as i64) * fx).div_euclid(256);
                    (t + ((u - t) * fy).div_euclid(256)) as u8
                }));
            }
        }
    }
    Picture::new(new_w, new_h, out)
}

/// The gradient the editor draws its canvas with: the colours of levels 0 to 255, the colour above
/// the clipping range and the colour below it.,
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gradient {
    pub levels: Vec<[u8; 3]>,
    pub above: [u8; 3],
    pub below: [u8; 3],
    /// The terrain's gradient shift as a whole number mod 256 for a terrain's own gradient; 0 for
    /// the default. Index k takes level (k - shift) mod 257 (256 read as 255).
    pub shift: i64,
}

impl Gradient {
    /// The default gradient from the substrate table terrain-default-gradient-three: 258 colours of
    /// three bytes, levels 0 to 255, then the colour above the range, then the one below it.
    pub fn from_table(table: &[u8]) -> Option<Gradient> {
        if table.len() != 774 {
            return None;
        }
        let colour = |k: usize| [table[3 * k], table[3 * k + 1], table[3 * k + 2]];
        Some(Gradient { levels: (0..256).map(colour).collect(), above: colour(256), below: colour(257), shift: 0 })
    }

    /// A terrain's own gradient: 256 level colours, red above the range and blue below it, the
    /// terrain's shift as a whole number mod 256.
    pub fn own(levels: Vec<[u8; 3]>, shift: i64) -> Gradient {
        assert_eq!(levels.len(), 256);
        Gradient { levels, above: [255, 0, 0], below: [0, 0, 255], shift: shift.rem_euclid(256) }
    }

    /// The colour the editor draws index `k` (0 to 255) with, the clipping bracket's lower and upper
    /// ends `lo` and `hi` being fractions of the height range.
    pub fn colour(&self, k: usize, lo: f64, hi: f64) -> [u8; 3] {
        let k = k as i64;
        let above = (256.0 * (1.0 - hi)).trunc() as i64;
        let below = (256.0 * (1.0 - lo)).trunc() as i64 + 2;
        let shade = |c: [u8; 3], m: i64| c.map(|c| ((c as i64 * m + 255).div_euclid(256)) as u8);
        if 256 - k < above {
            return shade(self.above, (k + 44).min(255));
        }
        if 256 - k > below {
            return shade(self.below, (k + 94).min(255));
        }
        let level = (k - self.shift).rem_euclid(257).min(255);
        self.levels[level as usize]
    }
}

/// P2: the canvas picture, 256 by 256 pixels, of a terrain of side `n` with the heights `heights`
/// (row after row, storage order).
pub fn canvas_picture(heights: &[u16], n: usize, gradient: &Gradient, lo: f64, hi: f64) -> Picture {
    let at = |r: usize, c: usize| heights[r * n + c] as i64;
    let mut out = Vec::with_capacity(256 * 256);
    if n < 256 {
        let s = (256 * n as i64 - 256) / 255;
        for i in 0..256i64 {
            let y = i * s;
            let (y0, fy) = (y / 256, y % 256);
            for j in 0..256i64 {
                let x = j * s;
                let (x0, fx) = (x / 256, x % 256);
                let row = |r: i64| {
                    let r = r as usize;
                    let v0 = at(r, x0 as usize);
                    if x0 as usize == n - 1 {
                        v0
                    } else {
                        v0 + ((at(r, x0 as usize + 1) - v0) * fx).div_euclid(256)
                    }
                };
                let t = row(n as i64 - 1 - y0);
                let l = if fy == 0 || n as i64 - 2 - y0 < 0 {
                    t
                } else {
                    let u = row(n as i64 - 2 - y0);
                    t + ((u - t) * fy).div_euclid(256)
                };
                out.push(gradient.colour((l.div_euclid(256)) as usize, lo, hi));
            }
        }
    } else {
        let k = (n - 1) / 255;
        for i in 0..256 {
            for j in 0..256 {
                let v = at(n - 1 - k * i, k * j);
                out.push(gradient.colour((v / 256) as usize, lo, hi));
            }
        }
    }
    Picture::new(256, 256, out)
}

/// The two versions the rules of this module cover.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edition {
    V30,
    V31,
}

/// One slot: the 70 by 70 picture it shows and the picture of its own (a loaded one; as 3.1 also a
/// pasted one) that Apply uses instead.
#[derive(Clone, Debug)]
pub struct Slot {
    pub shown: Picture,
    pub own: Option<Picture>,
}

const SLOT: usize = 70;

/// Why a command did nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TabError {
    /// The substrate lacks terrain-default-gradient-three: the command changes nothing and says which
    /// table is missing.
    MissingTable(&'static str),
}

impl std::fmt::Display for TabError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TabError::MissingTable(t) => write!(f, "the substrate lacks the table {t}, which the Pictures tab's canvas picture needs"),
        }
    }
}

/// The Pictures tab opened on a terrain.
pub struct Tab<'a> {
    edition: Edition,
    heights: &'a [u16],
    n: usize,
    pub slot_a: Slot,
    pub slot_b: Slot,
    pub clipboard: Option<Picture>,
    /// The Blend control's setting, 0 to 256; 128 as the tab opens.
    pub control: i64,
    pub mode: ResultMode,
}

/// Which slot a command is under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    A,
    B,
}

const WHITE: [u8; 3] = [255, 255, 255];

impl<'a> Tab<'a> {
    /// Opening the tab: slot A holds the canvas picture brought to 70 by 70 by P1 (blocks of 3 by
    /// 3), slot B white. The canvas needs the default gradient: with none the tab does not open
    /// its first slot (the command changes nothing).
    pub fn open(
        edition: Edition,
        heights: &'a [u16],
        n: usize,
        gradient: Option<&Gradient>,
        lo: f64,
        hi: f64,
    ) -> Result<Tab<'a>, TabError> {
        let g = gradient.ok_or(TabError::MissingTable("terrain-default-gradient-three"))?;
        let canvas = canvas_picture(heights, n, g, lo, hi);
        Ok(Tab::with_slot_a(edition, heights, n, resize(&canvas, SLOT, SLOT)))
    }

    /// The tab with slot A holding the given 70 by 70 picture (no gradient needed where nothing
    /// uses the canvas picture: a picture loaded or pasted into slot A).
    pub fn with_slot_a(edition: Edition, heights: &'a [u16], n: usize, a: Picture) -> Tab<'a> {
        Tab {
            edition,
            heights,
            n,
            slot_a: Slot { shown: a, own: None },
            slot_b: Slot { shown: Picture::flat(SLOT, SLOT, WHITE), own: None },
            clipboard: None,
            control: 128,
            mode: ResultMode::Blend,
        }
    }

    fn slot_mut(&mut self, which: Which) -> &mut Slot {
        match which {
            Which::A => &mut self.slot_a,
            Which::B => &mut self.slot_b,
        }
    }

    /// Copy under a slot. As 3.0 the slot's 70 by 70 picture goes on the clipboard. As 3.1 a slot
    /// that holds a picture of its own puts that picture on the clipboard at its own size, and a
    /// slot that holds none puts a picture of the terrain itself, n by n, each pixel the gray
    /// cut down(v / 256) of the height v (the grid's row 0 at the picture's bottom).
    pub fn copy(&mut self, which: Which) {
        let slot = match which {
            Which::A => &self.slot_a,
            Which::B => &self.slot_b,
        };
        self.clipboard = Some(match self.edition {
            Edition::V30 => slot.shown.clone(),
            Edition::V31 => match &slot.own {
                Some(p) => p.clone(),
                None => {
                    let n = self.n;
                    let px = (0..n * n)
                        .map(|k| {
                            let (i, c) = (k / n, k % n);
                            let v = self.heights[(n - 1 - i) * n + c];
                            let g = (v / 256) as u8;
                            [g, g, g]
                        })
                        .collect();
                    Picture::new(n, n, px)
                }
            },
        });
    }

    /// Paste under a slot: the clipboard's picture is brought into the slot's 70 by 70 picture by
    /// P1 (a 70 by 70 copy unchanged) and, as 3.1, kept as the slot's own picture. With nothing on
    /// the clipboard nothing changes.
    pub fn paste(&mut self, which: Which) {
        let Some(c) = self.clipboard.clone() else { return };
        let edition = self.edition;
        let slot = self.slot_mut(which);
        slot.shown = resize(&c, SLOT, SLOT);
        if edition == Edition::V31 {
            slot.own = Some(c);
        }
    }

    /// Load under a slot: the picture loaded, in its own colours, becomes the slot's own picture; the
    /// slot shows it brought to 70 by 70 by P1 and made gray.
    pub fn load(&mut self, which: Which, picture: Picture) {
        let mut shown = resize(&picture, SLOT, SLOT);
        for px in &mut shown.pixels {
            let g = gray_of(*px);
            *px = [g, g, g];
        }
        let slot = self.slot_mut(which);
        slot.shown = shown;
        slot.own = Some(picture);
    }

    /// Apply: each slot's own picture if it has one, else its 70 by 70 picture, is brought to n by n
    /// by P1; each colour channel of the result is the menu item's form at the control's setting;
    /// the gray is g = cut down((77 R + 151 G + 28 B) / 256) and the height at row r and column c is
    /// cut down(65535 g / 256) of the result's pixel in row n - 1 - r and column c.
    pub fn apply(&self) -> Vec<u16> {
        let n = self.n;
        let side = |s: &Slot| resize(s.own.as_ref().unwrap_or(&s.shown), n, n);
        let (a, b) = (side(&self.slot_a), side(&self.slot_b));
        let mut out = vec![0u16; n * n];
        for r in 0..n {
            for c in 0..n {
                let (pa, pb) = (a.at(n - 1 - r, c), b.at(n - 1 - r, c));
                let res: [u8; 3] = std::array::from_fn(|k| {
                    result_value(self.mode, pa[k] as i64, pb[k] as i64, self.control) as u8
                });
                let g = gray_of(res) as i64;
                out[r * n + c] = (65535 * g / 256) as u16;
            }
        }
        out
    }
}

/// g = cut down((77 R + 151 G + 28 B) / 256).
pub fn gray_of(p: [u8; 3]) -> u8 {
    ((77 * p[0] as i64 + 151 * p[1] as i64 + 28 * p[2] as i64) / 256) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P1 leaves a picture of the same size as it is.
    #[test]
    fn same_size_is_unchanged() {
        let p = Picture::gray(2, 2, &[1, 2, 3, 4]);
        assert_eq!(resize(&p, 2, 2), p);
    }

    /// P1: blocks of 2 by 2 from 4 by 4 to 2 by 2: sx = cut down((4 - 2) 65536 / 1) = 131072, so the
    /// blocks start at columns 0 and 2; each channel cut down(S m / 16777216), m = cut down(16843009 / 4).
    #[test]
    fn block_average() {
        let levels: Vec<u8> = (0..16).map(|k| (k * 10) as u8).collect();
        let p = Picture::gray(4, 4, &levels);
        let q = resize(&p, 2, 2);
        let m = 16_843_009i64 / 4;
        let want = |a: i64| ((a * m) / 16_777_216) as u8;
        assert_eq!(q.pixels[0][0], want(0 + 10 + 40 + 50));
        assert_eq!(q.pixels[3][0], want(100 + 110 + 140 + 150));
    }

    /// P1: bilinear growth does not quite reach the far corners: 2 to 5 gives sx = cut down(65535 / 4)
    /// = 16383, so column 4 has x0 = 0 and fx = 255 (t = 0 + cut down(100 · 255 / 256) = 99), and the
    /// same in rows.
    #[test]
    fn bilinear_by_sixteenths_of_a_pixel() {
        let p = Picture::gray(2, 2, &[0, 100, 200, 250]);
        let q = resize(&p, 5, 5);
        assert_eq!(q.pixels[0][0], 0);
        assert_eq!(q.pixels[4][0], 99);
        assert_eq!(q.pixels[20][0], 199);
        // t = 99, u = 200 + cut down(50 · 255 / 256) = 249, 99 + cut down(150 · 255 / 256) = 248
        assert_eq!(q.pixels[24][0], 248);
    }

    /// The default gradient's table gives level k as the (k + 1)th colour, above the range the 257th
    /// and below it the 258th.,
    #[test]
    fn gradient_from_table() {
        let table: Vec<u8> = (0..774).map(|k| (k % 251) as u8).collect();
        let g = Gradient::from_table(&table).unwrap();
        assert_eq!(g.levels[0], [0, 1, 2]);
        assert_eq!(g.levels[255], [(765 % 251) as u8, (766 % 251) as u8, (767 % 251) as u8]);
        assert_eq!(g.above, [(768 % 251) as u8, (769 % 251) as u8, (770 % 251) as u8]);
        assert!(Gradient::from_table(&table[..773]).is_none());
        // with the bracket at the whole range no index takes a shade
        assert_eq!(g.colour(10, 0.0, 1.0), g.levels[10]);
    }

    /// P2: a flat terrain of side 128 gives a picture of one index everywhere; the canvas of a ramp
    /// of side 256 or more takes every k-th point with row 0 at the grid's last row.
    #[test]
    fn canvas_indices() {
        let table: Vec<u8> = (0..774).map(|k| (k / 3) as u8).collect();
        let g = Gradient::from_table(&table).unwrap();
        let flat = vec![10000u16; 128 * 128];
        let c = canvas_picture(&flat, 128, &g, 0.0, 1.0);
        assert!(c.pixels.iter().all(|&p| p == [(10000 / 256) as u8; 3]));
        // side 512: k = 2, row i at the grid's row 511 - 2 i, column 2 j; height = 128 row
        let n = 512;
        let heights: Vec<u16> = (0..n * n).map(|k| ((k / n) * 128) as u16).collect();
        let c = canvas_picture(&heights, n, &g, 0.0, 1.0);
        assert_eq!(c.pixels[0], [((511 * 128) / 256) as u8 as u8; 3]);
    }

    /// 3.1: Copy under slot A, Paste under both slots and Apply: every height v becomes
    /// cut down(65535 cut down(v / 256) / 256) and nothing needs the canvas colours.
    /// (Variants: te5d-12, te5d-13)
    #[test]
    fn copy_paste_both_slots_as_31() {
        let n = 16;
        let heights: Vec<u16> = (0..n * n).map(|k| (k * 251 + 7) as u16).collect();
        let mut tab = Tab::with_slot_a(Edition::V31, &heights, n, Picture::flat(70, 70, [9, 9, 9]));
        tab.copy(Which::A);
        tab.paste(Which::A);
        tab.paste(Which::B);
        let got = tab.apply();
        for (g, &v) in got.iter().zip(&heights) {
            assert_eq!(*g as i64, 65535 * (v as i64 / 256) / 256);
        }
    }

    /// With no table the first slot cannot be made and the command says which table is missing.
    #[test]
    fn missing_table_is_reported() {
        let heights = vec![0u16; 16];
        let e = Tab::open(Edition::V30, &heights, 4, None, 0.0, 1.0).err().unwrap();
        assert!(e.to_string().contains("terrain-default-gradient-three"));
    }
}
