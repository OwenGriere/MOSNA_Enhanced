//! Gives `INSTALLATION.exe` its manifest and its icon.
//!
//! The manifest is what keeps Windows from running the installer as an
//! administrator (see `windows/setup.manifest`); the icon is what makes it
//! recognisable in Explorer. Both are Windows resources, written here as a
//! `.res` file — a simple format, so no resource compiler is needed on any
//! machine that builds this — and handed to the linker, which both Microsoft's
//! `link.exe` and LLD accept as they are.

use std::path::{Path, PathBuf};

const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
const RT_MANIFEST: u16 = 24;
/// English (United States), what resource compilers default to.
const LANGUAGE: u16 = 0x0409;
/// The sizes Explorer asks for, at every display scaling, largest first.
const ICON_SIZES: [u32; 9] = [256, 128, 64, 48, 40, 32, 24, 20, 16];

fn main() {
    let manifest = Path::new("windows/setup.manifest");
    let logo = Path::new("../../assets/logo.ico");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rerun-if-changed={}", logo.display());

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let mut resources = Resources::default();
    resources.add(
        RT_MANIFEST,
        1,
        0x0030,
        std::fs::read(manifest).expect("the manifest is missing"),
    );

    // The logo is a PNG whatever its extension says, and is decoded by content:
    // the `.ico` extension would select a decoder that does not apply.
    let logo = image::ImageReader::open(logo)
        .ok()
        .and_then(|reader| reader.with_guessed_format().ok())
        .and_then(|reader| reader.decode().ok());
    if let Some(logo) = logo {
        let mut group = vec![0, 0, 1, 0];
        group.extend((ICON_SIZES.len() as u16).to_le_bytes());
        for (index, size) in ICON_SIZES.into_iter().enumerate() {
            let id = index as u16 + 1;
            let pixels = logo
                .resize_exact(size, size, image::imageops::FilterType::Lanczos3)
                .into_rgba8();
            // Windows reads PNG entries reliably only at 256 pixels; below
            // that, some views of Explorer expect the older bitmap form.
            let data = if size >= 256 {
                let mut png = Vec::new();
                pixels
                    .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                    .expect("cannot encode the icon");
                png
            } else {
                bitmap(&pixels)
            };
            // GRPICONDIRENTRY: a width or height of 256 is written as 0.
            let side = if size >= 256 { 0 } else { size as u8 };
            group.extend([side, side, 0, 0]);
            group.extend(1u16.to_le_bytes());
            group.extend(32u16.to_le_bytes());
            group.extend((data.len() as u32).to_le_bytes());
            group.extend(id.to_le_bytes());
            resources.add(RT_ICON, id, 0x1010, data);
        }
        resources.add(RT_GROUP_ICON, 1, 0x1030, group);
    }

    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("mosna-setup.res");
    std::fs::write(&out, resources.bytes).expect("cannot write the resource file");
    println!("cargo:rustc-link-arg-bins={}", out.display());
}

/// A `.res` file: an empty entry, then one header and one payload per resource.
struct Resources {
    bytes: Vec<u8>,
}

impl Default for Resources {
    fn default() -> Self {
        let mut resources = Self { bytes: Vec::new() };
        // Every .res file starts with an empty resource of type and name 0.
        resources.entry(0, 0, 0, 0, &[]);
        resources
    }
}

impl Resources {
    fn add(&mut self, kind: u16, id: u16, flags: u16, data: Vec<u8>) {
        self.entry(kind, id, flags, LANGUAGE, &data);
    }

    fn entry(&mut self, kind: u16, id: u16, flags: u16, language: u16, data: &[u8]) {
        let bytes = &mut self.bytes;
        bytes.extend((data.len() as u32).to_le_bytes()); // DataSize
        bytes.extend(32u32.to_le_bytes()); // HeaderSize
        bytes.extend([0xFF, 0xFF]); // type, by number
        bytes.extend(kind.to_le_bytes());
        bytes.extend([0xFF, 0xFF]); // name, by number
        bytes.extend(id.to_le_bytes());
        bytes.extend(0u32.to_le_bytes()); // DataVersion
        bytes.extend(flags.to_le_bytes()); // MemoryFlags
        bytes.extend(language.to_le_bytes());
        bytes.extend(0u32.to_le_bytes()); // Version
        bytes.extend(0u32.to_le_bytes()); // Characteristics
        bytes.extend(data);
        while bytes.len() % 4 != 0 {
            bytes.push(0);
        }
    }
}

/// An icon image in bitmap form: a BITMAPINFOHEADER whose height counts the
/// colour rows and the mask rows together, the BGRA rows bottom-up, then a
/// one-bit mask left empty — the alpha channel already says what shows.
fn bitmap(pixels: &image::RgbaImage) -> Vec<u8> {
    let (width, height) = pixels.dimensions();
    let mask_row = (width.div_ceil(32) * 4) as usize;
    let mut data = Vec::new();
    data.extend(40u32.to_le_bytes()); // biSize
    data.extend((width as i32).to_le_bytes());
    data.extend((2 * height as i32).to_le_bytes());
    data.extend(1u16.to_le_bytes()); // biPlanes
    data.extend(32u16.to_le_bytes()); // biBitCount
    data.extend(0u32.to_le_bytes()); // BI_RGB
    data.extend((width * height * 4).to_le_bytes()); // biSizeImage
    data.extend([0; 16]); // resolution and palette: unused
    for y in (0..height).rev() {
        for x in 0..width {
            let [r, g, b, a] = pixels.get_pixel(x, y).0;
            data.extend([b, g, r, a]);
        }
    }
    data.extend(std::iter::repeat_n(0, mask_row * height as usize));
    data
}
