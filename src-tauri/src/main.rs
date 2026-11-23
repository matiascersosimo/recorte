#![windows_subsystem = "windows"]

mod inference;
mod models;
mod selection;

use serde::Serialize;
use std::{path::PathBuf, sync::{Arc, Mutex}};
use tauri::{ipc::Channel, AppHandle, Manager, State};

#[derive(Default)]
struct Workspace {
    selection: Arc<Mutex<Option<selection::Selection>>>,
    operation: Arc<Mutex<()>>,
    output: Arc<Mutex<Option<tempfile::NamedTempFile>>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Setup {
    sam_installed: bool,
    sam_bytes: u64,
    models: Vec<ModelStatus>,
    memory_gib: f64,
    arch: &'static str,
    logical_cores: usize,
    threads: usize,
    recommended: &'static str,
}

#[derive(Serialize)]
struct ModelStatus {
    #[serde(flatten)]
    model: models::Model,
    installed: bool,
}

fn cores() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

fn model_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_local_data_dir().map_err(|error| error.to_string())?.join("models"))
}

#[tauri::command]
fn setup(app: AppHandle) -> Result<Setup, String> {
    let dir = model_dir(&app)?;
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    let memory_gib = system.total_memory() as f64 / 1_073_741_824.0;
    let logical_cores = cores();
    Ok(Setup {
        sam_installed: models::SAM_FILES.iter().all(|f| f.installed(&dir)),
        sam_bytes: models::SAM_FILES.iter().map(|f| f.bytes).sum(),
        models: models::MODELS.into_iter().map(|model| ModelStatus {
            installed: model.installed(&dir), model,
        }).collect(),
        memory_gib,
        arch: std::env::consts::ARCH,
        logical_cores,
        threads: logical_cores.min(4),
        recommended: models::recommended(memory_gib, logical_cores),
    })
}

async fn blocking<T: Send + 'static>(task: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(task).await.map_err(|error| error.to_string())?
}

#[tauri::command]
async fn manage_model(app: AppHandle, state: State<'_, Workspace>, id: String, install: bool, progress: Channel<u64>) -> Result<(), String> {
    let dir = model_dir(&app)?;
    let cache = state.selection.clone();
    let operation = state.operation.clone();
    blocking(move || {
        let _guard = operation.lock().map_err(|error| error.to_string())?;
        if id == "mobilesam" {
            let mut offset = 0;
            for file in models::SAM_FILES {
                if install {
                    file.install(&dir, |bytes| { let _ = progress.send(offset + bytes); })?;
                    offset += file.bytes;
                    let _ = progress.send(offset);
                } else {
                    cache.lock().map_err(|e| e.to_string())?.take();
                    file.remove(&dir)?;
                }
            }
            return Ok(());
        }
        let model = models::get(&id)?;
        if install {
            model.install(&dir, |bytes| { let _ = progress.send(bytes); })
        } else {
            model.remove(&dir)
        }
    }).await
}

#[tauri::command]
async fn preview_image(path: PathBuf) -> Result<String, String> {
    blocking(move || inference::preview(&inference::read(&path)?)).await
}

#[tauri::command]
async fn process_image(app: AppHandle, state: State<'_, Workspace>, path: PathBuf, id: String, progress: Channel<&'static str>) -> Result<String, String> {
    let model = models::get(&id)?;
    let model_path = model.path(&model_dir(&app)?);
    let operation = state.operation.clone();
    let output = state.output.clone();
    blocking(move || {
        let _guard = operation.lock().map_err(|error| error.to_string())?;
        model.verify(&model_path)?;
        let result = inference::cutout(&model_path, model.profile, inference::read(&path)?, cores().min(4), |stage| {
            let _ = progress.send(stage);
        })?;
        save_result(result, &output)
    }).await
}

fn save_result(result: image::DynamicImage, output: &Mutex<Option<tempfile::NamedTempFile>>) -> Result<String, String> {
        let preview = inference::preview(&result)?;
        let mut staged = tempfile::NamedTempFile::new().map_err(|error| error.to_string())?;
        result.write_to(staged.as_file_mut(), image::ImageFormat::Png).map_err(|error| error.to_string())?;
        *output.lock().map_err(|error| error.to_string())? = Some(staged);
        Ok(preview)
}

#[tauri::command]
fn clear_selection(state: State<'_, Workspace>) -> Result<(), String> {
    state.selection.lock().map_err(|e| e.to_string())?.take();
    Ok(())
}

#[tauri::command]
async fn select_object(app: AppHandle, state: State<'_, Workspace>, path: PathBuf, points: Vec<selection::Point>) -> Result<String,String> {
    let dir=model_dir(&app)?;
    let cache=state.selection.clone();
    let operation=state.operation.clone();
    blocking(move || {
        let _guard=operation.lock().map_err(|e|e.to_string())?;
        let mut cache=cache.lock().map_err(|e|e.to_string())?;
        if cache.as_ref().map(|s| &s.path)!=Some(&path) {
            *cache=Some(selection::Selection::open(&dir,&path,cores().min(4))?);
        }
        let result=cache.as_mut().ok_or("Falta la imagen.")?.cutout(&points)?;
        inference::preview(&result)
    }).await
}

#[tauri::command]
async fn export_image(state: State<'_, Workspace>, path: PathBuf, interactive: Option<bool>) -> Result<(), String> {
    let output = state.output.clone();
    let selection = state.selection.clone();
    blocking(move || {
        let parent = path.parent().ok_or("Destino inválido.")?;
        let mut staged = tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
        if interactive.unwrap_or(false) {
            let selection = selection.lock().map_err(|e|e.to_string())?;
            let result = selection.as_ref().ok_or("Seleccioná un objeto.")?.original_cutout()?;
            result.write_to(staged.as_file_mut(), image::ImageFormat::Png).map_err(|e|e.to_string())?;
        } else {
            let output = output.lock().map_err(|error| error.to_string())?;
            let source = output.as_ref().ok_or("Todavía no hay un resultado para exportar.")?;
            let mut file = source.reopen().map_err(|error| error.to_string())?;
            std::io::copy(&mut file, staged.as_file_mut()).map_err(|error| error.to_string())?;
        }
        staged.persist(path).map_err(|error| error.to_string())?;
        Ok(())
    }).await
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Workspace::default())
        .invoke_handler(tauri::generate_handler![setup, manage_model, preview_image, process_image, export_image, select_object, clear_selection])
        .run(tauri::generate_context!())
        .expect("No se pudo iniciar Recorte");
}
