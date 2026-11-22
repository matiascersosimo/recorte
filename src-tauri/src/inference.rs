use image::{imageops::FilterType, DynamicImage, GrayImage, ImageDecoder, ImageReader, Limits};
use ort::{session::{run_options::{OutputSelector, RunOptions}, Session}, value::Tensor};
use std::{io::Cursor, path::Path};

use crate::models::Profile;

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

pub fn cutout(model: &Path, profile: Profile, image: DynamicImage, threads: usize, mut stage: impl FnMut(&'static str)) -> Result<DynamicImage, String> {
    let side = match profile {
        Profile::U2Net => 320usize,
        Profile::IsNet | Profile::BiRefNet => 1024,
    };
    let sample = image.resize_exact(side as u32, side as u32, FilterType::Lanczos3).into_rgb8();
    let divisor = f32::from(sample.as_raw().iter().copied().max().unwrap_or(0).max(1));
    let (mean, std) = match profile {
        Profile::IsNet => ([0.5; 3], [1.0; 3]),
        Profile::U2Net | Profile::BiRefNet => ([0.485, 0.456, 0.406], [0.229, 0.224, 0.225]),
    };
    let plane = side * side;
    let mut input = vec![0.0_f32; 3 * plane];
    for (index, pixel) in sample.pixels().enumerate() {
        for channel in 0..3 {
            input[channel * plane + index] =
                (f32::from(pixel[channel]) / divisor - mean[channel]) / std[channel];
        }
    }
    let mask = {
        let mut session = Session::builder().map_err(message)?
            .with_intra_threads(threads).map_err(message)?
            .commit_from_file(model).map_err(message)?;
        let primary = session.outputs.first().ok_or("El modelo no tiene salidas.")?.name.clone();
        let options = RunOptions::new().map_err(message)?
            .with_outputs(OutputSelector::no_default().with(primary));
        let tensor = Tensor::from_array(([1, 3, side, side], input)).map_err(message)?;
        stage("mask");
        let outputs = session.run_with_options(ort::inputs![tensor], &options).map_err(message)?;
        let (shape, scores) = outputs[0].try_extract_tensor::<f32>().map_err(message)?;
        if shape[..] != [1, 1, side as i64, side as i64] || scores.iter().any(|v| !v.is_finite()) {
            return Err("El modelo devolvió una máscara inválida.".into());
        }
        let probability: fn(f32) -> f32 = match profile {
            Profile::BiRefNet => |logit| 1.0 / (1.0 + (-logit).exp()),
            Profile::U2Net | Profile::IsNet => |score| score,
        };
        let min = probability(scores.iter().copied().fold(f32::INFINITY, f32::min));
        let max = probability(scores.iter().copied().fold(f32::NEG_INFINITY, f32::max));
        if max - min <= f32::EPSILON {
            return Err("El modelo no pudo distinguir un primer plano en esta imagen.".into());
        }
        let alpha = scores.iter().map(|&score| ((probability(score) - min) / (max - min) * 255.0).round() as u8).collect();
        GrayImage::from_raw(side as u32, side as u32, alpha).ok_or("Máscara inválida.")?
    };
    stage("compose");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::MODELS;

    #[test]
    #[ignore = "requiere una foto y los modelos descargados"]
    fn catalog_models_preserve_resolution_and_export_alpha() {
        let dir = std::env::var_os("RECORTE_TEST_MODELS").expect("RECORTE_TEST_MODELS");
        let input = std::env::var_os("RECORTE_TEST_IMAGE").expect("RECORTE_TEST_IMAGE");
        let source = read(Path::new(&input)).unwrap();
        for model in MODELS {
            let path = model.path(Path::new(&dir));
            model.verify(&path).unwrap();
            let start = std::time::Instant::now();
            let result = cutout(&path, model.profile, source.clone(), 4, |_| {}).unwrap().into_rgba8();
            assert_eq!(result.dimensions(), (source.width(), source.height()), "{}", model.id);
            assert!(result.pixels().any(|pixel| pixel[3] < 16), "{}", model.id);
            assert!(result.pixels().any(|pixel| pixel[3] > 240), "{}", model.id);
            println!("{}: {:.2?}", model.id, start.elapsed());
        }
    }
}
