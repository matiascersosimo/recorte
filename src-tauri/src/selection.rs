use crate::{inference, models::SAM_FILES};
use image::{imageops::FilterType, DynamicImage, GrayImage};
use ort::{session::Session, value::Tensor};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
    pub include: bool,
}

pub struct Selection {
    pub path: PathBuf,
    source: DynamicImage,
    preview: DynamicImage,
    mask: Option<GrayImage>,
    decoder: Session,
    embedding: Tensor<f32>,
    width: u32,
    height: u32,
}

fn session(path: &Path, threads: usize) -> Result<Session, String> {
    Session::builder()
        .map_err(message)?
        .with_intra_threads(threads)
        .map_err(message)?
        .commit_from_file(path)
        .map_err(message)
}

impl Selection {
    pub fn open(dir: &Path, path: &Path, threads: usize) -> Result<Self, String> {
        for file in SAM_FILES {
            file.verify(&file.path(dir))?;
        }
        let source = inference::read(path)?;
        let scale = 1024.0 / source.width().max(source.height()) as f64;
        let width = (source.width() as f64 * scale).round().max(1.0) as u32;
        let height = (source.height() as f64 * scale).round().max(1.0) as u32;
        let embedding = {
            let pixels = source
                .resize_exact(width, height, FilterType::Triangle)
                .into_rgb8();
            let rgb = pixels
                .into_raw()
                .into_iter()
                .map(f32::from)
                .collect::<Vec<_>>();
            let input =
                Tensor::from_array(([height as usize, width as usize, 3], rgb)).map_err(message)?;
            let mut encoder = session(&SAM_FILES[0].path(dir), threads)?;
            let outputs = encoder.run(ort::inputs![input]).map_err(message)?;
            let (shape, data) = outputs[0].try_extract_tensor::<f32>().map_err(message)?;
            Tensor::from_array((shape.to_vec(), data.to_vec())).map_err(message)?
        };
        let preview = source.thumbnail(850, 650);
        Ok(Self {
            path: path.into(),
            source,
            preview,
            mask: None,
            decoder: session(&SAM_FILES[1].path(dir), threads)?,
            embedding,
            width,
            height,
        })
    }

    pub fn cutout(&mut self, points: &[Point]) -> Result<DynamicImage, String> {
        if points.len() > 128
            || !points.iter().any(|p| p.include)
            || points
                .iter()
                .any(|p| !(0.0..=1.0).contains(&p.x) || !(0.0..=1.0).contains(&p.y))
        {
            return Err("Marcá al menos un punto para incluir. El límite es de 128 puntos.".into());
        }
        let mut coords = Vec::new();
        let mut labels = Vec::new();
        for p in points {
            coords.extend([p.x * self.width as f32, p.y * self.height as f32]);
            labels.push(if p.include { 1.0_f32 } else { 0.0 });
        }
        coords.extend([0.0, 0.0]);
        labels.push(-1.0);
        let n = labels.len();
        let inputs = ort::inputs![
            "image_embeddings" => &self.embedding,
            "point_coords" => Tensor::from_array(([1,n,2],coords)).map_err(message)?,
            "point_labels" => Tensor::from_array(([1,n],labels)).map_err(message)?,
            "mask_input" => Tensor::from_array(([1,1,256,256],vec![0.0_f32;256*256])).map_err(message)?,
            "has_mask_input" => Tensor::from_array(([1],vec![0.0_f32])).map_err(message)?,
            "orig_im_size" => Tensor::from_array(([2],vec![self.height as f32,self.width as f32])).map_err(message)?
        ];
        let outputs = self.decoder.run(inputs).map_err(message)?;
        let (shape, logits) = outputs
            .get("masks")
            .ok_or("Falta la máscara de MobileSAM.")?
            .try_extract_tensor::<f32>()
            .map_err(message)?;
        if shape[..] != [1, 1, self.height as i64, self.width as i64] {
            return Err("Tamaño de máscara inesperado.".into());
        }
        let alpha = logits
            .iter()
            .map(|&v| if v > 0.0 { 255 } else { 0 })
            .collect();
        let mask =
            GrayImage::from_raw(self.width, self.height, alpha).ok_or("Máscara inválida.")?;
        let preview = apply_mask(&self.preview, &mask);
        self.mask = Some(mask);
        Ok(preview)
    }

    pub fn original_cutout(&self) -> Result<DynamicImage, String> {
        Ok(apply_mask(
            &self.source,
            self.mask.as_ref().ok_or("Marcá un objeto primero.")?,
        ))
    }
}

fn apply_mask(image: &DynamicImage, mask: &GrayImage) -> DynamicImage {
    let mask = image::imageops::resize(mask, image.width(), image.height(), FilterType::Triangle);
    let mut result = image.to_rgba8();
    for (p, a) in result.pixels_mut().zip(mask.pixels()) {
        p[3] = ((u16::from(p[3]) * u16::from(a[0]) + 127) / 255) as u8;
    }
    result.into()
}

fn message(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requiere MobileSAM y la foto de prueba"]
    fn interactive_points_change_alpha() {
        let dir = std::env::var_os("RECORTE_SAM_DIR").unwrap();
        let photo = std::env::var_os("RECORTE_SAM_PHOTO").unwrap();
        let mut s = Selection::open(Path::new(&dir), Path::new(&photo), 4).unwrap();
        let mut points = vec![Point {
            x: 0.43,
            y: 0.23,
            include: true,
        }];
        let first = s.cutout(&points).unwrap().into_rgba8();
        points.push(Point {
            x: 0.58,
            y: 0.78,
            include: false,
        });
        let second = s.cutout(&points).unwrap().into_rgba8();
        assert_eq!(
            s.original_cutout().unwrap().into_rgba8().dimensions(),
            (s.source.width(), s.source.height())
        );
        assert!(second.pixels().any(|p| p[3] == 0) && second.pixels().any(|p| p[3] == 255));
        assert!(first
            .pixels()
            .zip(second.pixels())
            .any(|(a, b)| a[3] != b[3]));
    }
}
