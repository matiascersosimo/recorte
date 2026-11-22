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
    pub id: &'static str,
    pub name: &'static str,
    pub bytes: u64,
    #[serde(skip)]
    checksum: &'static str,
}

pub const MODELS: [Model; 2] = [
    Model {
        id: "u2netp",
        name: "U²-Net ligero",
        bytes: 4_574_861,
        checksum: "309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8",
    },
    Model {
        id: "u2net",
        name: "U²-Net completo",
        bytes: 175_997_641,
        checksum: "8d10d2f3bb75ae3b6d527c77944fc5e7dcd94b29809d47a739a7a728a912b491",
    },
];

pub fn get(id: &str) -> Result<Model, String> {
    MODELS.iter().copied().find(|m| m.id == id).ok_or_else(|| "Modelo desconocido.".into())
}

impl Model {
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

    pub fn install(self, dir: &Path) -> Result<(), String> {
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
        let url = format!("https://github.com/danielgatis/rembg/releases/download/v0.0.0/{}.onnx", self.id);
        let response = client.get(url).send().and_then(|r| r.error_for_status()).map_err(message)?;
        let written = std::io::copy(&mut response.take(self.bytes + 1), staged.as_file_mut()).map_err(message)?;
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
