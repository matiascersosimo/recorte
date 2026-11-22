import { Channel, invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, save } from "@tauri-apps/plugin-dialog";
import "./style.css";
import { animateEdges } from "./edges.js";

const byId = (id) => document.getElementById(id);
const ui = {
  open: byId("open"), run: byId("run"), save: byId("save"),
  drop: byId("drop"), comparison: byId("comparison"), empty: byId("empty"),
  before: byId("before"), after: byId("after"), edges: byId("edges"),
  layer: byId("output-layer"), split: byId("split"), divider: byId("divider"),
  leftTag: byId("left-tag"), rightTag: byId("right-tag"),
  filename: byId("filename"), status: byId("status"), models: byId("models"),
  hardware: byId("hardware"), recommendation: byId("recommendation"),
  progressPanel: byId("progress-panel"), progress: byId("progress"),
  progressLabel: byId("progress-label"), progressValue: byId("progress-value"),
};
const formats = [{ name: "Imágenes", extensions: ["png", "jpg", "jpeg", "webp"] }];
const desktop = isTauri();
let configuration;
let selected;
let source;
let result = false;
let busy = false;

function comparisonMode(visible) {
  for (const element of [ui.layer, ui.divider, ui.split, ui.rightTag]) element.hidden = !visible;
}
function compare(value) {
  ui.split.value = value;
  ui.comparison.style.setProperty("--split", value + "%");
}
ui.split.addEventListener("input", () => compare(ui.split.value));

function status(message, error = false) {
  ui.status.textContent = message;
  ui.status.classList.toggle("error", error);
}

function showProgress(label, percent) {
  if (!busy) return;
  const opening = ui.progressPanel.hidden;
  ui.progressPanel.hidden = false;
  if (opening || ui.progressLabel.textContent !== label) {
    ui.progressLabel.textContent = label;
    status(label);
  }
  ui.progressValue.textContent = percent === undefined ? "" : `${percent} %`;
  if (percent === undefined) ui.progress.removeAttribute("value");
  else ui.progress.value = percent;
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
    ui.progressPanel.hidden = true;
    ui.comparison.classList.remove("processing");
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
    const quality = document.createElement("p");
    quality.textContent = model.detail;
    const action = document.createElement("button");
    action.className = "secondary";
    action.textContent = model.installed ? "Eliminar" : "Descargar";
    action.setAttribute("aria-label", `${action.textContent} ${model.name}`);
    action.addEventListener("click", () => perform(model.installed ? "Eliminando modelo…" : "", async () => {
      const progress = new Channel();
      if (!model.installed) {
        const label = `Descargando ${model.name}…`;
        showProgress(label, 0);
        progress.onmessage = (bytes) => {
          const percent = Math.floor(bytes * 100 / model.bytes);
          if (percent === 100) showProgress("Verificando el modelo…");
          else showProgress(label, percent);
        };
      }
      await invoke("manage_model", { id: model.id, install: !model.installed, progress });
      if (!model.installed) selected = model.id;
      await loadCatalog();
      status(model.installed ? "Modelo eliminado." : "Modelo instalado. Ya podés usarlo sin conexión.");
    }));
    card.append(label, quality, details, action);
    ui.models.append(card);
  }
  updateControls();
}

async function loadCatalog() {
  configuration = await invoke("setup");
  const suggested = configuration.models.find((model) => model.id === configuration.recommended);
  selected ??= suggested.installed
    ? suggested.id
    : configuration.models.find((model) => model.installed)?.id ?? suggested.id;
  const { memoryGib, logicalCores, threads, arch } = configuration;
  ui.hardware.textContent = `${memoryGib.toFixed(1)} GiB de RAM · ${logicalCores} núcleos lógicos · ${arch}`;
  ui.recommendation.textContent = `${suggested.name} sugerido. Procesamiento por CPU con hasta ${threads} hilos.`;
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
  comparisonMode(false);
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

ui.run.addEventListener("click", () => perform("", async () => {
  const progress = new Channel();
  const stages = { mask: "Calculando la máscara…", compose: "Preparando el PNG transparente…" };
  showProgress("Preparando la imagen y el modelo…");
  await ui.before.decode();
  const stop = animateEdges(ui.edges, ui.before);
  comparisonMode(false);
  ui.leftTag.textContent = "Mapa de bordes";
  try {
    progress.onmessage = (stage) => showProgress(stages[stage]);
    const preview = await invoke("process_image", { path: source, id: selected, progress });
    ui.after.src = preview;
    await ui.after.decode();
  } finally {
    stop();
    ui.leftTag.textContent = "Original";
    comparisonMode(result);
  }
  compare(50);
  comparisonMode(true);
  if (!matchMedia("(prefers-reduced-motion: reduce)").matches) {
    const reveal = ui.layer.animate([{clipPath:"inset(0 0 0 100%)"}, {clipPath:"inset(0 0 0 50%)"}], {duration:550,easing:"ease-out"});
    ui.split.onpointerdown = () => reveal.cancel();
  }
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
