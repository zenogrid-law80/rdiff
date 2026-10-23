use std::{
    env,
    error::Error,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use image::ImageEncoder;

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=website/favicon.png");
    let target_os = env::var("CARGO_CFG_TARGET_OS")?;
    if !matches!(target_os.as_str(), "windows" | "macos") {
        return Ok(());
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is missing")?);
    let source = image::load_from_memory_with_format(
        &fs::read("website/favicon.png")?,
        image::ImageFormat::Png,
    )?;
    if target_os == "macos" {
        let directory = out_dir.join("rdiff.iconset");
        fs::create_dir_all(&directory)?;
        for size in [16, 32, 128, 256, 512] {
            for scale in [1, 2] {
                let suffix = if scale == 2 { "@2x" } else { "" };
                fs::write(
                    directory.join(format!("icon_{size}x{size}{suffix}.png")),
                    render_icon(&source, size * scale)?,
                )?;
            }
        }
        return Ok(());
    }

    embed_windows_icon(&source, &out_dir)
}

fn render_icon(source: &image::DynamicImage, size: u32) -> Result<Vec<u8>, Box<dyn Error>> {
    let scaled = source
        .resize(size, size, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    // Preserve the PNG's aspect ratio and center it on a transparent square.
    let mut pixels = image::RgbaImage::new(size, size);
    image::imageops::replace(
        &mut pixels,
        &scaled,
        ((size - scaled.width()) / 2).into(),
        ((size - scaled.height()) / 2).into(),
    );
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png).write_image(
        pixels.as_raw(),
        size,
        size,
        image::ExtendedColorType::Rgba8,
    )?;
    Ok(png)
}

fn embed_windows_icon(source: &image::DynamicImage, out_dir: &Path) -> Result<(), Box<dyn Error>> {
    let sizes = [16u32, 24, 32, 48, 64, 128, 256];
    let mut images = Vec::new();
    for size in sizes {
        images.push(render_icon(source, size)?);
    }

    // ICO directory followed by PNG frames, preserving alpha transparency.
    let mut icon = Vec::new();
    icon.write_all(&0u16.to_le_bytes())?; // Reserved.
    icon.write_all(&1u16.to_le_bytes())?; // Icon, rather than cursor.
    icon.write_all(&(sizes.len() as u16).to_le_bytes())?;
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (size, image) in sizes.into_iter().zip(&images) {
        // An ICO dimension byte of zero represents 256 pixels.
        icon.write_all(&[size as u8, size as u8, 0, 0])?;
        icon.write_all(&1u16.to_le_bytes())?; // Color planes.
        icon.write_all(&32u16.to_le_bytes())?; // RGBA bits per pixel.
        icon.write_all(&(image.len() as u32).to_le_bytes())?;
        icon.write_all(&offset.to_le_bytes())?;
        offset += image.len() as u32;
    }
    for image in images {
        icon.write_all(&image)?;
    }
    let icon_path = out_dir.join("rdiff.ico");
    fs::write(&icon_path, icon)?;

    // GPUI loads resource 1 for the window and taskbar icon on Windows.
    let resource_path = out_dir.join("rdiff.rc");
    fs::write(
        &resource_path,
        format!(
            "1 ICON \"{}\"\n",
            icon_path.to_string_lossy().replace('\\', "/")
        ),
    )?;
    embed_resource::compile_for(&resource_path, ["rdiff"], embed_resource::NONE)
        .manifest_required()?;
    Ok(())
}
