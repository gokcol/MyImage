//! Export Windows Store artwork from MyImage's existing calico icon.
use std::{error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let out = PathBuf::from(std::env::args_os().nth(1).ok_or("Pass an output folder")?);
    std::fs::create_dir_all(&out)?;
    let icon = image::load_from_memory(include_bytes!(
        "../assets/branding/myimage-app-icon-256.png"
    ))?;
    for (name, base) in [
        ("StoreLogo", 50),
        ("Square44x44Logo", 44),
        ("Square150x150Logo", 150),
    ] {
        for scale in [100, 125, 150, 200, 400] {
            let size = (base * scale + 50) / 100;
            let file = if scale == 100 {
                format!("{name}.png")
            } else {
                format!("{name}.scale-{scale}.png")
            };
            icon.resize_exact(size, size, image::imageops::FilterType::Lanczos3)
                .save(out.join(file))?;
        }
    }
    for size in [16, 24, 32, 44, 48, 256] {
        icon.resize_exact(size, size, image::imageops::FilterType::Lanczos3)
            .save(out.join(format!(
                "Square44x44Logo.targetsize-{size}_altform-unplated.png"
            )))?;
    }
    Ok(())
}
