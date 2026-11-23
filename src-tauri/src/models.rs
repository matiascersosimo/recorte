use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{BufReader, Read, Write},
    path::Path,
    time::Duration,
};

#[derive(Clone, Copy, Serialize)]
pub struct Model {
    pub name: &'static str,
    pub detail: &'static str,
    #[serde(skip)]
    pub profile: Profile,
    #[serde(flatten)]
    file: ModelFile,
}

impl std::ops::Deref for Model {
    type Target = ModelFile;
    fn deref(&self) -> &ModelFile { &self.file }
}

#[derive(Clone, Copy, Serialize)]
pub struct ModelFile {
    pub id: &'static str,
    pub bytes: u64,
    #[serde(skip)]
    asset: &'static str,
    #[serde(skip)]
    checksum: &'static str,
}

#[derive(Clone, Copy)]
pub enum Profile {
    U2Net,
    IsNet,
    BiRefNet,
}

// These files form a single user-visible model. Keep them outside the automatic catalog.
pub const SAM_FILES: [ModelFile; 2] = [
    ModelFile {
        id: "mobile_sam_image_encoder", bytes: 28_157_093,
        asset: "https://huggingface.co/t9749/mobilesam-onnx/resolve/5c378f4561c10ef3af2c798cac4447b9adc62e63/mobile_sam_image_encoder.onnx",
        checksum: "580f5fb648ea1062c0aabc26217aed56921985f03f0cbbd852bba81d760cc749",
    },
    ModelFile {
        id: "sam_mask_decoder_single", bytes: 16_501_323,
        asset: "https://huggingface.co/t9749/mobilesam-onnx/resolve/5c378f4561c10ef3af2c798cac4447b9adc62e63/sam_mask_decoder_single.onnx",
        checksum: "93915fc7c993ab9d59ab8c9ccd3bce37f7509c81ab4150a74abd4d2abbd8570d",
    },
];

pub const MODELS: [Model; 4] = [
    Model {
        name: "U²-Net ligero",
        detail: "Rápido · 320 × 320",
        profile: Profile::U2Net,
        file: ModelFile {
            id: "u2netp",
            bytes: 4_574_861,
            asset: "u2netp.onnx",
            checksum: "309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8",
        },
    },
    Model {
        name: "U²-Net completo",
        detail: "General · 320 × 320",
        profile: Profile::U2Net,
        file: ModelFile {
            id: "u2net",
            bytes: 175_997_641,
            asset: "u2net.onnx",
            checksum: "8d10d2f3bb75ae3b6d527c77944fc5e7dcd94b29809d47a739a7a728a912b491",
        },
    },
    Model {
        name: "IS-Net",
        detail: "Alta fidelidad · 1024 × 1024",
        profile: Profile::IsNet,
        file: ModelFile {
            id: "isnet-general-use",
            bytes: 178_648_008,
            asset: "isnet-general-use.onnx",
            checksum: "60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a",
        },
    },
    Model {
        name: "BiRefNet Lite",
        detail: "Alta fidelidad · 1024 × 1024 · Mayor carga",
        profile: Profile::BiRefNet,
        file: ModelFile {
            id: "birefnet-lite",
            bytes: 224_005_088,
            asset: "BiRefNet-general-bb_swin_v1_tiny-epoch_232.onnx",
            checksum: "5600024376f572a557870a5eb0afb1e5961636bef4e1e22132025467d0f03333",
        },
    },
];

pub fn recommended(memory_gib: f64, logical_cores: usize) -> &'static str {
    if memory_gib >= 15.0 && logical_cores >= 8 {
        "birefnet-lite"
    } else if memory_gib >= 8.0 && logical_cores >= 4 {
        "isnet-general-use"
    } else {
        "u2netp"
    }
}

pub fn get(id: &str) -> Result<Model, String> {
    MODELS.iter().copied().find(|m| m.id == id).ok_or_else(|| "Modelo desconocido.".into())
}

impl ModelFile {
    pub fn path(self, dir: &Path) -> std::path::PathBuf {
        dir.join(format!("{}.onnx", self.id))
    }

    pub fn installed(self, dir: &Path) -> bool {
        self.path(dir).metadata().is_ok_and(|metadata| metadata.is_file() && metadata.len() == self.bytes)
    }

    pub fn verify(self, path: &Path) -> Result<(), String> {
        let mut file = BufReader::new(File::open(path).map_err(message)?);
        let mut hash = Sha256::new();
        let mut buffer = [0; 64 * 1024];
        loop {
            let count = file.read(&mut buffer).map_err(message)?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
        if format!("{:x}", hash.finalize()) != self.checksum {
            return Err("El archivo del modelo no coincide con la versión esperada. Eliminá el modelo y volvé a descargarlo.".into());
        }
        Ok(())
    }

    pub fn install(self, dir: &Path, mut progress: impl FnMut(u64)) -> Result<(), String> {
        let path = self.path(dir);
        if self.installed(dir) && self.verify(&path).is_ok() {
            return Ok(());
        }
        fs::create_dir_all(dir).map_err(message)?;
        let mut staged = tempfile::NamedTempFile::new_in(dir).map_err(message)?;
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(1_800))
            .build().map_err(message)?;
        let url = if self.asset.starts_with("https://") { self.asset.to_owned() } else {
            format!("https://github.com/danielgatis/rembg/releases/download/v0.0.0/{}", self.asset)
        };
        let response = client.get(url).send().and_then(|r| r.error_for_status()).map_err(message)?;
        let mut response = response.take(self.bytes + 1);
        let mut buffer = [0; 64 * 1024];
        let mut written = 0;
        let mut last_percent = 0;
        loop {
            let count = response.read(&mut buffer).map_err(message)?;
            if count == 0 {
                break;
            }
            staged.as_file_mut().write_all(&buffer[..count]).map_err(message)?;
            written += count as u64;
            let percent = written * 100 / self.bytes;
            if percent > last_percent {
                progress(written.min(self.bytes));
                last_percent = percent;
            }
        }
        if written != self.bytes {
            return Err("La descarga tiene un tamaño inesperado.".into());
        }
        staged.as_file_mut().flush().map_err(message)?;
        self.verify(staged.path())?;
        staged.persist(path).map_err(message)?;
        Ok(())
    }

    pub fn remove(self, dir: &Path) -> Result<(), String> {
        match fs::remove_file(self.path(dir)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(message(error)),
        }
    }
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}
