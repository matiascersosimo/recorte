use image::{imageops::FilterType, DynamicImage, GrayImage, ImageDecoder, ImageReader, Limits};
use ort::{session::Session, value::Tensor};
use std::{io::Cursor, path::Path};

const SIDE: usize = 320;

pub fn read(path: &Path) -> Result<DynamicImage, String> {
    let mut reader = ImageReader::open(path).map_err(message)?.with_guessed_format().map_err(message)?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(160 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(message)?;
    if decoder.total_bytes() > 128 * 1024 * 1024 {
        return Err("La imagen supera el límite de tamaño del MVP.".into());
    }
    let orientation = decoder.orientation().map_err(message)?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(message)?;
    image.apply_orientation(orientation);
    Ok(image)
}

pub fn preview(image: &DynamicImage) -> Result<String, String> {
    use base64::Engine;
    let mut png = Cursor::new(Vec::new());
    image.thumbnail(850, 650).write_to(&mut png, image::ImageFormat::Png).map_err(message)?;
    Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png.into_inner())))
}

pub fn cutout(model: &Path, image: DynamicImage, threads: usize) -> Result<DynamicImage, String> {
    let sample = image.resize_exact(SIDE as u32, SIDE as u32, FilterType::Lanczos3).into_rgb8();
    let divisor = f32::from(sample.as_raw().iter().copied().max().unwrap_or(0).max(1));
    let mean = [0.485, 0.456, 0.406];
    let std = [0.229, 0.224, 0.225];
    let mut input = vec![0.0_f32; 3 * SIDE * SIDE];
    for (index, pixel) in sample.pixels().enumerate() {
        for channel in 0..3 {
            input[channel * SIDE * SIDE + index] =
                (f32::from(pixel[channel]) / divisor - mean[channel]) / std[channel];
        }
    }
    let mask = {
        let mut session = Session::builder().map_err(message)?
            .with_intra_threads(threads).map_err(message)?
            .commit_from_file(model).map_err(message)?;
        let tensor = Tensor::from_array(([1, 3, SIDE, SIDE], input)).map_err(message)?;
        let outputs = session.run(ort::inputs![tensor]).map_err(message)?;
        let (shape, scores) = outputs[0].try_extract_tensor::<f32>().map_err(message)?;
        if shape[..] != [1, 1, SIDE as i64, SIDE as i64] || scores.iter().any(|v| !v.is_finite()) {
            return Err("El modelo devolvió una máscara inválida.".into());
        }
        let min = scores.iter().copied().fold(f32::INFINITY, f32::min);
        let max = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        if max - min <= f32::EPSILON {
            return Err("El modelo no pudo distinguir un primer plano en esta imagen.".into());
        }
        let alpha = scores.iter().map(|score| ((score - min) / (max - min) * 255.0).round() as u8).collect();
        GrayImage::from_raw(SIDE as u32, SIDE as u32, alpha).ok_or("Máscara inválida.")?
    };
    let mut result = image.into_rgba8();
    let mask = image::imageops::resize(&mask, result.width(), result.height(), FilterType::Triangle);
    for (pixel, alpha) in result.pixels_mut().zip(mask.pixels()) {
        pixel[3] = ((u16::from(pixel[3]) * u16::from(alpha[0]) + 127) / 255) as u8;
    }
    Ok(result.into())
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}
