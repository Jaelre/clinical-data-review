use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

fn generate_compile_icon() -> PathBuf {
    let output_directory = PathBuf::from(
        std::env::var_os("OUT_DIR").expect("Cargo must provide OUT_DIR to the Tauri build script"),
    );
    let icon_path = output_directory.join("compile-icon.png");
    let file = File::create(&icon_path).expect("failed to create generated compile icon");
    let mut encoder = png::Encoder::new(BufWriter::new(file), 32, 32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .expect("failed to initialize generated compile icon");

    let mut pixels = Vec::with_capacity(32 * 32 * 4);
    for y in 0..32 {
        for x in 0..32 {
            let color = if (8..24).contains(&x) && (6..26).contains(&y) {
                [247, 250, 252, 255]
            } else {
                [23, 50, 77, 255]
            };
            pixels.extend_from_slice(&color);
        }
    }
    writer
        .write_image_data(&pixels)
        .expect("failed to write generated compile icon");
    icon_path
}

fn main() {
    if std::env::var_os("TAURI_CONFIG").is_none() {
        let icon_path = generate_compile_icon();
        let escaped_path = icon_path
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        let manifest_directory = PathBuf::from(
            std::env::var_os("CARGO_MANIFEST_DIR")
                .expect("Cargo must provide CARGO_MANIFEST_DIR to the Tauri build script"),
        );
        let frontend_distribution = manifest_directory.join("../dist");
        let config = if frontend_distribution.is_dir() {
            format!("{{\"bundle\":{{\"icon\":[\"{escaped_path}\"]}}}}")
        } else {
            if std::env::var("PROFILE").as_deref() == Ok("release") {
                panic!("apps/review/dist is missing; build the Vite UI before a release build");
            }
            format!(
                "{{\"build\":{{\"frontendDist\":\"../ui\"}},\"bundle\":{{\"icon\":[\"{escaped_path}\"]}}}}"
            )
        };
        println!("cargo:rustc-env=TAURI_CONFIG={config}");
    }
    tauri_build::build();
}
