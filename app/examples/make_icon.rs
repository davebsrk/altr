//! Renders assets/altr.svg into assets/altr.ico (every Windows size) and assets/altr.png (256 px).
//! Run from the workspace root: `cargo run -p altr --example make_icon`.

use image::codecs::ico::{IcoEncoder, IcoFrame};
use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
use resvg::{tiny_skia, usvg};
use std::fs::File;

fn render(tree: &usvg::Tree, size: u32) -> Vec<u8> {
    let mut pixmap = tiny_skia::Pixmap::new(size, size).unwrap();
    let scale = size as f32 / tree.size().width();
    resvg::render(tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    // tiny-skia stores premultiplied RGBA; icons want straight alpha.
    pixmap.pixels().iter().flat_map(|p| { let c = p.demultiply(); [c.red(), c.green(), c.blue(), c.alpha()] }).collect()
}

fn main() {
    let svg = std::fs::read_to_string("assets/altr.svg").unwrap();
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).unwrap();

    let frames: Vec<IcoFrame> = [16, 20, 24, 32, 40, 48, 64, 128, 256]
        .into_iter()
        .map(|size| IcoFrame::as_png(&render(&tree, size), size, size, ExtendedColorType::Rgba8).unwrap())
        .collect();
    IcoEncoder::new(File::create("assets/altr.ico").unwrap()).encode_images(&frames).unwrap();

    PngEncoder::new(File::create("assets/altr.png").unwrap())
        .write_image(&render(&tree, 256), 256, 256, ExtendedColorType::Rgba8)
        .unwrap();
    println!("assets/altr.ico and assets/altr.png written");
}
