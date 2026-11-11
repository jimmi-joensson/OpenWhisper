//! Pixel work on a captured screen, all of it in memory: black out
//! rectangles, scale the image down, encode it as PNG.
//!
//! Nothing here knows about windows or apps. The capture
//! ([`super::capture_for_ask`]) hands in an RGBA buffer and the window
//! rectangles of the apps that must not be seen; this module makes
//! sure no pixel of those rectangles survives.
//!
//! The order matters and is fixed by the caller: paint at the
//! capture's own size first, scale down after. Scaling blends
//! neighbouring pixels, so painting a scaled image would leave a rim
//! of the window's own colour around the black. Painted first, the
//! rim is a blend of black and whatever is next to the window.
//!
//! Buffers are RGBA, 8 bits per channel, rows from the top, no
//! padding: `width * height * 4` bytes.

use super::PointRect;

/// A painted rectangle is this many pixels larger on every side than
/// the window it covers: a window's shadow line and the rounding of
/// a fractional scale never leave an edge showing.
pub const GROW_PX: i64 = 2;

/// Longest side of the image that leaves the machine, in pixels.
/// Larger images are scaled down by the model's API anyway.
pub const MAX_SIDE: u32 = 1568;

/// A rectangle in image pixels: `x0..x1` by `y0..y1`, ends excluded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PxRect {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

/// Where `rect` (global points) lies in an image of `width` by
/// `height` pixels that shows `display` (global points).
///
/// Rounded outward (a pixel the window covers only half of is in),
/// grown by [`GROW_PX`] on every side and clipped to the image.
/// `None` when nothing of the rectangle is in the image, and when any
/// number involved is not finite or the display has no size.
pub fn to_pixels(rect: &PointRect, display: &PointRect, width: u32, height: u32) -> Option<PxRect> {
    if !rect.is_finite() || !display.is_finite() || display.width <= 0.0 || display.height <= 0.0 {
        return None;
    }
    if rect.width <= 0.0 || rect.height <= 0.0 || width == 0 || height == 0 {
        return None;
    }
    let sx = width as f64 / display.width;
    let sy = height as f64 / display.height;
    let grow = GROW_PX as f64;
    let clip = |v: f64, max: u32| v.clamp(0.0, max as f64) as u32;
    let x0 = clip(((rect.x - display.x) * sx).floor() - grow, width);
    let y0 = clip(((rect.y - display.y) * sy).floor() - grow, height);
    let x1 = clip(((rect.x + rect.width - display.x) * sx).ceil() + grow, width);
    let y1 = clip(((rect.y + rect.height - display.y) * sy).ceil() + grow, height);
    (x1 > x0 && y1 > y0).then_some(PxRect { x0, y0, x1, y1 })
}

/// Make every pixel of `rect` opaque black. A rectangle that reaches
/// past the image is clipped to it; a buffer that is too short for
/// `width` by `height` is left alone past its end.
pub fn paint_black(buf: &mut [u8], width: u32, height: u32, rect: PxRect) {
    let x0 = rect.x0.min(width) as usize;
    let x1 = rect.x1.min(width) as usize;
    let y0 = rect.y0.min(height) as usize;
    let y1 = rect.y1.min(height) as usize;
    if x1 <= x0 {
        return;
    }
    let stride = width as usize * 4;
    for y in y0..y1 {
        let row = y * stride;
        let Some(span) = buf.get_mut(row + x0 * 4..row + x1 * 4) else {
            return;
        };
        for px in span.chunks_exact_mut(4) {
            px.copy_from_slice(&[0, 0, 0, 255]);
        }
    }
}

/// For each of `to` output cells, the first input cell it covers and
/// how much of each covered cell it takes (the shares add up to 1).
fn spans(from: usize, to: usize) -> Vec<(usize, Vec<f32>)> {
    let step = from as f64 / to as f64;
    (0..to)
        .map(|i| {
            let lo = i as f64 * step;
            let hi = ((i + 1) as f64 * step).min(from as f64);
            let first = lo.floor() as usize;
            let last = (hi.ceil() as usize).clamp(first + 1, from);
            let mut shares: Vec<f32> = (first..last)
                .map(|cell| (hi.min(cell as f64 + 1.0) - lo.max(cell as f64)).max(0.0) as f32)
                .collect();
            let total: f32 = shares.iter().sum();
            if total > 0.0 {
                shares.iter_mut().for_each(|s| *s /= total);
            } else {
                shares.iter_mut().for_each(|s| *s = 1.0 / (last - first) as f32);
            }
            (first, shares)
        })
        .collect()
}

/// Scale an image down so that its longest side is at most
/// `max_side`, keeping its proportions. Each output pixel is the
/// average of the input area it covers, so nothing is skipped: a
/// black rectangle stays black right up to its rim. An image that is
/// already small enough is returned as it is. Returns the pixels and
/// the new width and height.
pub fn downscale(rgba: Vec<u8>, width: u32, height: u32, max_side: u32) -> (Vec<u8>, u32, u32) {
    let longest = width.max(height);
    if longest <= max_side || max_side == 0 || rgba.len() < width as usize * height as usize * 4 {
        return (rgba, width, height);
    }
    let k = max_side as f64 / longest as f64;
    let new_w = ((width as f64 * k).round() as u32).clamp(1, max_side);
    let new_h = ((height as f64 * k).round() as u32).clamp(1, max_side);
    let (w, nw, nh) = (width as usize, new_w as usize, new_h as usize);
    let columns = spans(w, nw);
    let rows = spans(height as usize, nh);

    let mut out = vec![0u8; nw * nh * 4];
    // One input row squeezed to the new width, and the output row
    // being summed up.
    let mut squeezed = vec![0f32; nw * 3];
    let mut sum = vec![0f32; nw * 3];
    for (y, (first_row, row_shares)) in rows.iter().enumerate() {
        sum.iter_mut().for_each(|v| *v = 0.0);
        for (i, row_share) in row_shares.iter().enumerate() {
            let line = &rgba[(first_row + i) * w * 4..(first_row + i + 1) * w * 4];
            for (x, (first_col, col_shares)) in columns.iter().enumerate() {
                let mut acc = [0f32; 3];
                for (j, share) in col_shares.iter().enumerate() {
                    let px = &line[(first_col + j) * 4..(first_col + j) * 4 + 3];
                    acc[0] += px[0] as f32 * share;
                    acc[1] += px[1] as f32 * share;
                    acc[2] += px[2] as f32 * share;
                }
                squeezed[x * 3..x * 3 + 3].copy_from_slice(&acc);
            }
            for (total, value) in sum.iter_mut().zip(&squeezed) {
                *total += value * row_share;
            }
        }
        let line = &mut out[y * nw * 4..(y + 1) * nw * 4];
        for (px, rgb) in line.chunks_exact_mut(4).zip(sum.chunks_exact(3)) {
            px[0] = (rgb[0] + 0.5).clamp(0.0, 255.0) as u8;
            px[1] = (rgb[1] + 0.5).clamp(0.0, 255.0) as u8;
            px[2] = (rgb[2] + 0.5).clamp(0.0, 255.0) as u8;
            px[3] = 255;
        }
    }
    // The full-size pixels are not needed any more.
    let mut rgba = rgba;
    rgba.fill(0);
    (out, new_w, new_h)
}

/// Encode the image as PNG, in memory, with fast compression (the
/// file is sent once and thrown away; speed matters more than size).
/// A screen has no transparency, so the alpha channel is left out.
pub fn encode_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let pixels = width as usize * height as usize;
    if width == 0 || height == 0 || rgba.len() != pixels * 4 {
        return Err("the image buffer does not match its size".to_string());
    }
    let mut rgb = Vec::with_capacity(pixels * 3);
    for px in rgba.chunks_exact(4) {
        rgb.extend_from_slice(&px[..3]);
    }
    let mut out = Vec::with_capacity(pixels / 2);
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder.write_header().map_err(|e| format!("png header: {e}"))?;
        writer.write_image_data(&rgb).map_err(|e| format!("png data: {e}"))?;
        writer.finish().map_err(|e| format!("png end: {e}"))?;
    }
    rgb.fill(0);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GREEN: [u8; 4] = [0, 200, 0, 255];
    const MAGENTA: [u8; 4] = [255, 0, 255, 255];
    const BLACK: [u8; 4] = [0, 0, 0, 255];

    fn solid(width: u32, height: u32, colour: [u8; 4]) -> Vec<u8> {
        colour.iter().copied().cycle().take(width as usize * height as usize * 4).collect()
    }

    fn fill(buf: &mut [u8], width: u32, rect: PxRect, colour: [u8; 4]) {
        for y in rect.y0..rect.y1 {
            for x in rect.x0..rect.x1 {
                let i = (y * width + x) as usize * 4;
                buf[i..i + 4].copy_from_slice(&colour);
            }
        }
    }

    fn at(buf: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let i = (y * width + x) as usize * 4;
        [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
    }

    fn pt(x: f64, y: f64, width: f64, height: f64) -> PointRect {
        PointRect { x, y, width, height }
    }

    /// Red and blue both high, green low: the window's own colour, or
    /// a blend that still shows it.
    fn magenta_ish(px: [u8; 4]) -> bool {
        px[0] > 40 && px[2] > 40 && px[1] < px[0]
    }

    #[test]
    fn points_map_to_pixels_rounded_outward_and_grown() {
        let display = pt(0.0, 0.0, 1000.0, 500.0);
        // 1x: one point is one pixel.
        assert_eq!(
            to_pixels(&pt(100.0, 50.0, 200.0, 100.0), &display, 1000, 500),
            Some(PxRect { x0: 98, y0: 48, x1: 302, y1: 152 })
        );
        // Fractions round away from the window on both sides.
        assert_eq!(
            to_pixels(&pt(100.4, 50.6, 200.2, 100.1), &display, 1000, 500),
            Some(PxRect { x0: 98, y0: 48, x1: 303, y1: 153 })
        );
        // Retina: two pixels per point.
        assert_eq!(
            to_pixels(&pt(100.0, 50.0, 200.0, 100.0), &display, 2000, 1000),
            Some(PxRect { x0: 198, y0: 98, x1: 602, y1: 302 })
        );
        assert_eq!(
            to_pixels(&pt(100.5, 50.5, 0.25, 0.25), &display, 2000, 1000),
            Some(PxRect { x0: 199, y0: 99, x1: 204, y1: 104 })
        );
        // A fractional scale (1.5) still rounds outward.
        assert_eq!(
            to_pixels(&pt(101.0, 51.0, 33.0, 33.0), &display, 1500, 750),
            Some(PxRect { x0: 149, y0: 74, x1: 203, y1: 128 })
        );
    }

    #[test]
    fn a_second_display_is_mapped_from_its_own_origin() {
        // A display to the right of the main one, and one above it.
        let right = pt(1440.0, 0.0, 1920.0, 1080.0);
        assert_eq!(
            to_pixels(&pt(1500.0, 100.0, 300.0, 200.0), &right, 1920, 1080),
            Some(PxRect { x0: 58, y0: 98, x1: 362, y1: 302 })
        );
        let above = pt(-200.0, -1080.0, 1920.0, 1080.0);
        assert_eq!(
            to_pixels(&pt(-100.0, -1000.0, 50.0, 50.0), &above, 3840, 2160),
            Some(PxRect { x0: 198, y0: 158, x1: 302, y1: 262 })
        );
        // A window on the main display is not in the right one's image.
        assert_eq!(to_pixels(&pt(100.0, 100.0, 300.0, 200.0), &right, 1920, 1080), None);
    }

    #[test]
    fn rectangles_are_clipped_to_the_image_and_nonsense_maps_to_nothing() {
        let display = pt(0.0, 0.0, 1000.0, 500.0);
        // Partly off every edge.
        assert_eq!(
            to_pixels(&pt(-50.0, -20.0, 100.0, 100.0), &display, 1000, 500),
            Some(PxRect { x0: 0, y0: 0, x1: 52, y1: 82 })
        );
        assert_eq!(
            to_pixels(&pt(950.0, 450.0, 400.0, 400.0), &display, 1000, 500),
            Some(PxRect { x0: 948, y0: 448, x1: 1000, y1: 500 })
        );
        // Larger than the display: the whole image.
        assert_eq!(
            to_pixels(&pt(-500.0, -500.0, 5000.0, 5000.0), &display, 2000, 1000),
            Some(PxRect { x0: 0, y0: 0, x1: 2000, y1: 1000 })
        );
        // Far outside, empty, or not a number.
        assert_eq!(to_pixels(&pt(3000.0, 100.0, 100.0, 100.0), &display, 1000, 500), None);
        assert_eq!(to_pixels(&pt(-900.0, 100.0, 100.0, 100.0), &display, 1000, 500), None);
        assert_eq!(to_pixels(&pt(10.0, 10.0, 0.0, 100.0), &display, 1000, 500), None);
        assert_eq!(to_pixels(&pt(10.0, 10.0, -5.0, 100.0), &display, 1000, 500), None);
        assert_eq!(to_pixels(&pt(f64::NAN, 10.0, 5.0, 100.0), &display, 1000, 500), None);
        assert_eq!(to_pixels(&pt(10.0, 10.0, f64::INFINITY, 100.0), &display, 1000, 500), None);
        assert_eq!(to_pixels(&pt(10.0, 10.0, 5.0, 5.0), &pt(0.0, 0.0, 0.0, 500.0), 1000, 500), None);
        assert_eq!(to_pixels(&pt(10.0, 10.0, 5.0, 5.0), &display, 0, 500), None);
    }

    #[test]
    fn paint_blacks_every_pixel_inside_and_none_outside() {
        let (w, h) = (64u32, 48u32);
        for rect in [
            PxRect { x0: 10, y0: 8, x1: 30, y1: 20 },
            PxRect { x0: 0, y0: 0, x1: 1, y1: 1 },
            PxRect { x0: 0, y0: 0, x1: w, y1: h },
            PxRect { x0: 60, y0: 40, x1: w, y1: h },
            PxRect { x0: 0, y0: 47, x1: w, y1: 48 },
        ] {
            let mut buf = solid(w, h, MAGENTA);
            paint_black(&mut buf, w, h, rect);
            for y in 0..h {
                for x in 0..w {
                    let inside = x >= rect.x0 && x < rect.x1 && y >= rect.y0 && y < rect.y1;
                    let want = if inside { BLACK } else { MAGENTA };
                    assert_eq!(at(&buf, w, x, y), want, "{rect:?} at {x},{y}");
                }
            }
        }
    }

    #[test]
    fn paint_clips_a_rectangle_that_reaches_past_the_image() {
        let (w, h) = (20u32, 10u32);
        let mut buf = solid(w, h, GREEN);
        paint_black(&mut buf, w, h, PxRect { x0: 15, y0: 6, x1: 500, y1: 900 });
        assert_eq!(at(&buf, w, 14, 6), GREEN);
        assert_eq!(at(&buf, w, 15, 5), GREEN);
        assert_eq!(at(&buf, w, 15, 6), BLACK);
        assert_eq!(at(&buf, w, 19, 9), BLACK);
        // Entirely outside, inverted, or on a short buffer: no panic,
        // and nothing painted that should not be.
        let mut buf = solid(w, h, GREEN);
        paint_black(&mut buf, w, h, PxRect { x0: 30, y0: 30, x1: 40, y1: 40 });
        paint_black(&mut buf, w, h, PxRect { x0: 8, y0: 8, x1: 2, y1: 2 });
        assert_eq!(buf, solid(w, h, GREEN));
        let mut short = solid(w, h - 2, GREEN);
        paint_black(&mut short, w, h, PxRect { x0: 0, y0: 0, x1: w, y1: h });
        assert_eq!(short, solid(w, h - 2, BLACK));
    }

    /// A magenta window on a green screen, captured at `scale` pixels
    /// per point, painted and scaled down: no trace of magenta.
    fn no_magenta_after(display: PointRect, scale: f64, window: PointRect, max_side: u32) {
        let width = (display.width * scale).round() as u32;
        let height = (display.height * scale).round() as u32;
        let mut buf = solid(width, height, GREEN);
        // The window as the screen would show it: every pixel it
        // touches at all is its colour.
        let touched = PxRect {
            x0: (((window.x - display.x) * scale).floor().max(0.0) as u32).min(width),
            y0: (((window.y - display.y) * scale).floor().max(0.0) as u32).min(height),
            x1: (((window.x + window.width - display.x) * scale).ceil().max(0.0) as u32).min(width),
            y1: (((window.y + window.height - display.y) * scale).ceil().max(0.0) as u32).min(height),
        };
        fill(&mut buf, width, touched, MAGENTA);
        assert!(buf.chunks_exact(4).any(|p| p == MAGENTA), "the window is not on the test screen");

        let rect = to_pixels(&window, &display, width, height).expect("the window is on the display");
        paint_black(&mut buf, width, height, rect);
        // At native size: nothing of the window is left at all.
        assert!(!buf.chunks_exact(4).any(|p| magenta_ish([p[0], p[1], p[2], p[3]])), "magenta after paint");
        for y in touched.y0..touched.y1 {
            for x in touched.x0..touched.x1 {
                assert_eq!(at(&buf, width, x, y), BLACK, "window pixel {x},{y} not black");
            }
        }
        // Well outside the window the screen is untouched.
        let far = 4;
        for (x, y) in [(touched.x0.wrapping_sub(far), touched.y0), (touched.x1 + far, touched.y1.saturating_sub(1))] {
            if x < width && y < height {
                assert_eq!(at(&buf, width, x, y), GREEN, "screen pixel {x},{y} was painted");
            }
        }

        let (small, w, h) = downscale(buf, width, height, max_side);
        assert!(w.max(h) <= max_side.max(1) || (width.max(height) <= max_side));
        assert_eq!(small.len(), w as usize * h as usize * 4);
        let mut darkest = u8::MAX;
        for px in small.chunks_exact(4) {
            let px = [px[0], px[1], px[2], px[3]];
            assert!(!magenta_ish(px), "a magenta-ish pixel survived the downscale: {px:?}");
            // Only green, black, or a blend of the two at the rim.
            assert_eq!((px[0], px[2], px[3]), (0, 0, 255), "{px:?}");
            assert!(px[1] <= GREEN[1], "{px:?}");
            darkest = darkest.min(px[1]);
        }
        // The painted rectangle is still there: black, or for a
        // sliver a few pixels wide, nearly so.
        assert!(darkest < GREEN[1] / 4, "the painted rectangle is gone after the downscale ({darkest})");
    }

    #[test]
    fn no_pixel_of_a_painted_window_survives_at_any_scale_or_position() {
        let display = pt(0.0, 0.0, 1512.0, 982.0);
        let windows = [
            pt(120.0, 140.0, 240.0, 160.0),
            pt(120.5, 140.25, 240.3, 160.7),
            pt(-80.0, -40.0, 300.0, 200.0),
            pt(1400.0, 900.0, 400.0, 300.0),
            pt(700.0, 0.0, 3.0, 982.0),
            pt(0.0, 0.0, 1512.0, 982.0),
        ];
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for window in windows {
                no_magenta_after(display, scale, window, MAX_SIDE);
                no_magenta_after(display, scale, window, 500);
            }
        }
        // A second display with its own origin, Retina.
        let second = pt(1512.0, -200.0, 2560.0, 1440.0);
        no_magenta_after(second, 2.0, pt(1600.0, 100.0, 800.0, 600.0), MAX_SIDE);
        no_magenta_after(second, 2.0, pt(1400.0, -300.0, 300.0, 300.0), MAX_SIDE);
    }

    #[test]
    fn downscale_keeps_the_proportions_and_the_colours() {
        // 3024 x 1964 (a Retina laptop) ends at 1568 wide.
        let (small, w, h) = downscale(solid(3024, 1964, GREEN), 3024, 1964, MAX_SIDE);
        assert_eq!((w, h), (1568, 1018));
        assert!(small.chunks_exact(4).all(|p| p == GREEN));
        // Portrait: the height is the long side.
        let (small, w, h) = downscale(solid(1000, 4000, MAGENTA), 1000, 4000, MAX_SIDE);
        assert_eq!((w, h), (392, 1568));
        assert!(small.chunks_exact(4).all(|p| p == MAGENTA));
        // Small enough already: the very same pixels.
        let original = solid(800, 600, GREEN);
        assert_eq!(downscale(original.clone(), 800, 600, MAX_SIDE), (original, 800, 600));
        // An exact 2:1: each output pixel is the average of four.
        let mut checker = solid(4, 2, [0, 0, 0, 255]);
        fill(&mut checker, 4, PxRect { x0: 0, y0: 0, x1: 1, y1: 1 }, [200, 100, 40, 255]);
        fill(&mut checker, 4, PxRect { x0: 2, y0: 0, x1: 4, y1: 2 }, [80, 80, 80, 255]);
        let (small, w, h) = downscale(checker, 4, 2, 2);
        assert_eq!((w, h), (2, 1));
        assert_eq!(small, [50, 25, 10, 255, 80, 80, 80, 255]);
    }

    #[test]
    fn spans_cover_every_input_cell_exactly_once() {
        for (from, to) in [(10usize, 4usize), (3024, 1568), (7, 7), (5, 1), (1964, 1018), (9, 2)] {
            let mut covered = vec![0f32; from];
            for (first, shares) in spans(from, to) {
                assert!((shares.iter().sum::<f32>() - 1.0).abs() < 1e-4);
                let width = from as f32 / to as f32;
                for (i, share) in shares.iter().enumerate() {
                    covered[first + i] += share * width;
                }
            }
            for (cell, total) in covered.iter().enumerate() {
                assert!((total - 1.0).abs() < 1e-2, "{from}->{to}: cell {cell} covered {total}");
            }
        }
    }

    #[test]
    fn png_is_made_in_memory_and_decodes_to_the_same_pixels() {
        let (w, h) = (40u32, 30u32);
        let mut buf = solid(w, h, GREEN);
        paint_black(&mut buf, w, h, PxRect { x0: 5, y0: 5, x1: 20, y1: 15 });
        let png = encode_png(&buf, w, h).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");

        let mut reader = png::Decoder::new(std::io::Cursor::new(&png)).read_info().unwrap();
        let info = reader.info();
        assert_eq!((info.width, info.height, info.color_type), (w, h, png::ColorType::Rgb));
        let mut rgb = vec![0u8; w as usize * h as usize * 3];
        reader.next_frame(&mut rgb).unwrap();
        for (i, px) in rgb.chunks_exact(3).enumerate() {
            assert_eq!(px, &buf[i * 4..i * 4 + 3]);
        }

        assert!(encode_png(&buf, w, h + 1).is_err());
        assert!(encode_png(&[], 0, 0).is_err());
    }
}
