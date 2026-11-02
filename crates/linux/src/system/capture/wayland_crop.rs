use std::io::Cursor;

use agent_desktop_core::{AdapterError, DisplayInfo, ErrorCode, ImageBuffer};
use png::{ColorType, Decoder, Transformations};
use serde_json::json;

use super::png_encode::encode_png;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PixelRect {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

pub(crate) fn unproven() -> AdapterError {
    AdapterError::new(
        ErrorCode::PlatformNotSupported,
        "The display's place in the Wayland screenshot could not be proven",
    )
    .with_suggestion("Read the app with 'snapshot' instead, or give every display the same scale")
    .with_details(json!({ "reason": "display_mapping" }))
}

fn extent(displays: &[DisplayInfo]) -> Option<(f64, f64, f64, f64)> {
    let first = displays.first()?;
    let start = (
        first.bounds.x,
        first.bounds.y,
        first.bounds.x,
        first.bounds.y,
    );
    Some(
        displays
            .iter()
            .fold(start, |(left, top, right, bottom), display| {
                (
                    left.min(display.bounds.x),
                    top.min(display.bounds.y),
                    right.max(display.bounds.x + display.bounds.width),
                    bottom.max(display.bounds.y + display.bounds.height),
                )
            }),
    )
}

fn whole(value: f64) -> Option<u32> {
    let rounded = value.round();
    ((rounded - value).abs() < 0.01 && (0.0..=f64::from(u32::MAX)).contains(&rounded))
        .then_some(rounded as u32)
}

fn nearest(value: f64) -> Option<u32> {
    let rounded = value.round();
    (0.0..=f64::from(u32::MAX))
        .contains(&rounded)
        .then_some(rounded as u32)
}

type Snap = fn(f64) -> Option<u32>;

fn uniform_scale(displays: &[DisplayInfo]) -> Option<f64> {
    let first = displays.first()?.scale;
    displays
        .iter()
        .all(|display| (display.scale - first).abs() < 1e-6)
        .then_some(first)
}

fn largest_scale(displays: &[DisplayInfo]) -> Option<f64> {
    displays
        .iter()
        .map(|display| display.scale)
        .reduce(f64::max)
}

fn rect_at(
    display: &DisplayInfo,
    origin: (f64, f64),
    factor: f64,
    image: (u32, u32),
    snap: Snap,
) -> Option<PixelRect> {
    let x = snap((display.bounds.x - origin.0) * factor)?;
    let y = snap((display.bounds.y - origin.1) * factor)?;
    let right = snap((display.bounds.x + display.bounds.width - origin.0) * factor)?;
    let bottom = snap((display.bounds.y + display.bounds.height - origin.1) * factor)?;
    let rect = PixelRect {
        x,
        y,
        width: right.checked_sub(x)?,
        height: bottom.checked_sub(y)?,
    };
    let inside = rect.width > 0 && rect.height > 0 && right <= image.0 && bottom <= image.1;
    inside.then_some(rect)
}

/// Where `display` lies in a whole-desktop image of `image` pixels, and how
/// many image pixels that part has per logical pixel. The reported rectangles
/// are in image pixels already, or in logical pixels that every display scales
/// by one common factor, or in logical pixels drawn at the largest display
/// scale and rounded to whole pixels, which is how Mutter's
/// `clutter_stage_get_capture_final_size` sizes a capture across mixed
/// scales. The first hypothesis whose union of rectangles has the image's size
/// is the one that holds, and no match is an error rather than a guess.
pub(crate) fn pixel_rect(
    display: &DisplayInfo,
    all: &[DisplayInfo],
    image: (u32, u32),
) -> Result<(PixelRect, f64), AdapterError> {
    let (left, top, right, bottom) = extent(all).ok_or_else(unproven)?;
    let exact = [Some(1.0), uniform_scale(all)]
        .into_iter()
        .flatten()
        .map(|factor| (factor, whole as Snap, display.scale));
    let drawn = largest_scale(all).map(|factor| (factor, nearest as Snap, factor));
    exact
        .chain(drawn)
        .find(|(factor, snap, _)| {
            snap((right - left) * factor) == Some(image.0)
                && snap((bottom - top) * factor) == Some(image.1)
        })
        .and_then(|(factor, snap, scale)| {
            rect_at(display, (left, top), factor, image, snap).map(|rect| (rect, scale))
        })
        .ok_or_else(unproven)
}

fn unreadable() -> AdapterError {
    AdapterError::new(
        ErrorCode::ActionFailed,
        "The portal's screenshot is not a PNG this tool can crop",
    )
}

/// The part of a PNG inside `rect`, encoded again as a PNG.
pub(crate) fn crop_png(
    bytes: &[u8],
    rect: PixelRect,
    scale_factor: f64,
) -> Result<ImageBuffer, AdapterError> {
    let mut decoder = Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(Transformations::EXPAND | Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|_| unreadable())?;
    let mut frame = vec![0; reader.output_buffer_size().ok_or_else(unreadable)?];
    let info = reader.next_frame(&mut frame).map_err(|_| unreadable())?;
    let channels = match info.color_type {
        ColorType::Rgb => 3,
        ColorType::Rgba => 4,
        _ => return Err(unreadable()),
    };
    let (width, height) = (info.width as usize, info.height as usize);
    let (x, y) = (rect.x as usize, rect.y as usize);
    let (cut_width, cut_height) = (rect.width as usize, rect.height as usize);
    if x + cut_width > width || y + cut_height > height {
        return Err(unreadable());
    }
    let mut rgb = Vec::with_capacity(cut_width * cut_height * 3);
    for row in y..y + cut_height {
        let line = &frame[row * info.line_size..][..width * channels];
        for pixel in line[x * channels..(x + cut_width) * channels].chunks_exact(channels) {
            rgb.extend_from_slice(&pixel[..3]);
        }
    }
    encode_png(&rgb, rect.width, rect.height, scale_factor)
}

#[cfg(test)]
#[path = "wayland_crop_tests.rs"]
mod tests;
