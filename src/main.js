import { invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, save } from "@tauri-apps/plugin-dialog";
import "./style.css";

const byId = (id) => document.getElementById(id);
const ui = {
  open: byId("open"), run: byId("run"), save: byId("save"),
  drop: byId("drop"), comparison: byId("comparison"), empty: byId("empty"),
  before: byId("before"), after: byId("after"), placeholder: byId("placeholder"),
  filename: byId("filename"), status: byId("status"), models: byId("models"),
  hardware: byId("hardware"), recommendation: byId("recommendation"),
};
const formats = [{ name: "Imágenes", extensions: ["png", "jpg", "jpeg", "webp"] }];
const desktop = isTauri();
let configuration;
let selected;
let source;
let result = false;
let busy = false;

function status(message, error = false) {
  ui.status.textContent = message;
  ui.status.classList.toggle("error", error);
}

function updateControls() {
  ui.open.disabled = busy || !desktop;
  ui.run.disabled = busy || !source || !configuration?.models.find((model) => model.id === selected)?.installed;
  ui.save.disabled = busy || !result;
  ui.models.querySelectorAll("button, input").forEach((element) => {
    element.disabled = busy;
  });
}

async function perform(message, operation) {
  if (busy) return;
  busy = true;
  status(message);
  updateControls();
  try {
    await operation();
  } catch (error) {
    status(error instanceof Error ? error.message : String(error), true);
  } finally {
    busy = false;
    updateControls();
  }
}

function renderCatalog() {
  ui.models.replaceChildren();
  for (const model of configuration.models) {
    const card = document.createElement("article");
    card.className = "model";
    const label = document.createElement("label");
    const radio = document.createElement("input");
    radio.type = "radio";
    radio.name = "model";
    radio.checked = model.id === selected;
    radio.addEventListener("change", () => {
      selected = model.id;
      updateControls();
    });
    const name = document.createElement("strong");
    name.textContent = model.name;
    label.append(radio, name);
    const details = document.createElement("p");
    details.textContent = `${(model.bytes / 1e6).toLocaleString("es", { maximumFractionDigits: 1 })} MB · ${model.installed ? "Instalado" : "Disponible"}`;
    const action = document.createElement("button");
    action.className = "secondary";
    action.textContent = model.installed ? "Eliminar" : "Descargar";
    action.setAttribute("aria-label", `${action.textContent} ${model.name}`);
    action.addEventListener("click", () => perform(model.installed ? "Eliminando modelo…" : "Descargando y verificando modelo…", async () => {
      await invoke("manage_model", { id: model.id, install: !model.installed });
      if (!model.installed) selected = model.id;
      await loadCatalog();
      status(model.installed ? "Modelo eliminado." : "Modelo instalado. Ya podés usarlo sin conexión.");
    }));
    card.append(label, details, action);
    ui.models.append(card);
  }
  updateControls();
}

async function loadCatalog() {
  configuration = await invoke("setup");
  selected ??= configuration.recommended;
  const { memoryGib, logicalCores, threads, arch, recommended } = configuration;
  ui.hardware.textContent = `${memoryGib.toFixed(1)} GiB de RAM · ${logicalCores} núcleos lógicos · ${arch}`;
  const modelName = configuration.models.find((model) => model.id === recommended).name;
  ui.recommendation.textContent = `${modelName} sugerido. Procesamiento por CPU con hasta ${threads} hilos.`;
  renderCatalog();
}

function filename(path) {
  return path.split(/[\\/]/).at(-1);
}

async function loadImage(path) {
  const preview = await invoke("preview_image", { path });
  source = path;
  result = false;
  ui.before.src = preview;
  ui.after.removeAttribute("src");
  ui.after.hidden = true;
  ui.placeholder.hidden = false;
  ui.filename.textContent = filename(path);
  ui.empty.hidden = true;
  ui.comparison.hidden = false;
  status("");
}

ui.open.addEventListener("click", () => perform("", async () => {
  const path = await open({ filters: formats, multiple: false });
  if (path) {
    status("Abriendo imagen…");
    await loadImage(path);
  }
}));

ui.run.addEventListener("click", () => perform("Quitando fondo en este equipo…", async () => {
  ui.after.src = await invoke("process_image", { path: source, id: selected });
  ui.after.hidden = false;
  ui.placeholder.hidden = true;
  result = true;
  status("El recorte está listo para exportar.");
}));

ui.save.addEventListener("click", () => perform("", async () => {
  const stem = filename(source).replace(/\.[^.]+$/, "");
  const path = await save({ defaultPath: `${stem}-sin-fondo.png`, filters: [{ name: "PNG", extensions: ["png"] }] });
  if (!path) return;
  const destination = /\.png$/i.test(path) ? path : `${path}.png`;
  await invoke("export_image", { path: destination });
  status(`Guardado en ${destination}`);
}));

updateControls();
if (desktop) {
  getCurrentWebview().onDragDropEvent(({ payload }) => {
    ui.drop.classList.toggle("dragging", !busy && ["enter", "over"].includes(payload.type));
    if (payload.type === "drop" && payload.paths.length && !busy) {
      if (payload.paths.length === 1) {
        void perform("Abriendo imagen…", () => loadImage(payload.paths[0]));
      } else {
        status("Arrastrá una sola imagen por vez.", true);
      }
    }
  }).catch((error) => status(String(error), true));
  void perform("", loadCatalog);
} else {
  ui.hardware.textContent = "Disponible en la aplicación de escritorio.";
  status("Vista previa de la interfaz. Abrí Recorte para procesar imágenes.");
}
