# Recorte

MVP de escritorio para quitar el fondo de una imagen en Windows x64. Acepta PNG, JPEG y WebP, muestra el antes y el después y exporta PNG con transparencia a la resolución de la imagen original. Se procesa una imagen por vez.

## Ejecutable actual

En la carpeta de trabajo se mantiene un único ejecutable de entrega, `outputs/Recorte.exe`. Corresponde a la última compilación publicada e incluye la selección por puntos con MobileSAM. El historial del código se conserva en Git.

Después de compilar, actualizá ese archivo con `scripts/update-executable.ps1 -BinaryPath <ruta-al-ejecutable-compilado>`. Las siguientes entregas reemplazan el mismo archivo. El script local `work/build-recorte.ps1` lo actualiza automáticamente al terminar una compilación correcta.

## Seleccionar un objeto

Abrí una foto y elegí **Seleccionar objeto**. La primera vez, descargá **MobileSAM** desde el panel lateral (44,7 MB). Con **Incluir (+)**, hacé clic dentro del objeto. Usá **Excluir (−)** sobre una zona que quieras retirar o **Deshacer punto** para corregir el último clic. La vista previa atenúa el resto de la foto. **Exportar PNG** guarda la selección con transparencia a la resolución original.

MobileSAM procesa la foto en CPU y reutiliza su representación en memoria para los siguientes puntos. Los dos archivos ONNX se descargan bajo demanda y se verifican con SHA-256. [Modelo MobileSAM](https://github.com/ChaoningZhang/MobileSAM) · [Exportación ONNX utilizada](https://huggingface.co/t9749/mobilesam-onnx/tree/5c378f4561c10ef3af2c798cac4447b9adc62e63)

## Desarrollo en Windows

Instalá Node.js compatible con Vite 7, Rust estable con el destino MSVC, Microsoft C++ Build Tools con el SDK de Windows y WebView2. Los requisitos y enlaces de instalación están en la [documentación de Tauri 2](https://v2.tauri.app/start/prerequisites/).

```powershell
npm ci
npm run tauri dev
npm run tauri build
```

El último comando prepara el instalador NSIS. Los pesos ONNX se descargan desde la aplicación. La primera compilación de Rust necesita Internet para descargar dependencias y el runtime de inferencia.

## Arquitectura

- **Tauri 2 y Vite**. La interfaz usa HTML, CSS y JavaScript, con diálogos y arrastre nativos de Tauri. En Windows usa WebView2. Elegí Tauri para dar forma a la interfaz con CSS y mantener la inferencia aislada en Rust.
- **Slint** ofrece UI declarativa con renderizado propio. Es una opción si se decide quitar WebView2 después de medirlo. **egui + eframe** permite desarrollar herramientas enteramente en Rust con un modelo de interfaz inmediata. El MVP adopta Tauri sin fijar dependencias del motor de inferencia con la interfaz.
- **Catálogo y caché** en `models.rs`. Descarga por HTTPS, valida el tamaño y el SHA-256 y recién entonces publica el archivo en la caché local de la aplicación. Los SHA-256 corresponden a los artefactos cotejados con los checksums de rembg.
- **Inferencia** en `inference.rs`. ONNX Runtime para CPU, con hasta cuatro hilos. Abre una sesión por recorte y la libera al obtener la máscara. La interfaz recibe vistas previas reducidas y el PNG completo queda temporalmente en disco para exportarlo.
- **Recomendación** por RAM total y núcleos lógicos accesibles. Sugiere BiRefNet Lite desde 15 GiB y 8 núcleos lógicos, IS-Net desde 8 GiB y 4 núcleos lógicos, y U²-Net ligero para el resto. Los umbrales son orientativos. El tiempo y la memoria varían según el equipo.
- **Modelos** [U²-Net](https://github.com/xuebinqin/U-2-Net) y U²-Net ligero procesan a 320 × 320. [IS-Net](https://github.com/xuebinqin/DIS) y [BiRefNet Lite](https://github.com/ZhengPeng7/BiRefNet) procesan a 1024 × 1024 para captar detalles más finos, con mayor costo de CPU y RAM. Los cuatro usan exportaciones ONNX de [rembg](https://github.com/danielgatis/rembg/releases/tag/v0.0.0). Después de descargar un modelo, abrir, procesar y exportar funciona localmente.

## Verificación de escritorio

Abrí una foto, descargá U²-Net ligero y compará las dos vistas. Exportá el PNG y revisá dimensiones y canal alfa en un editor de imágenes. Repetí con IS-Net y BiRefNet Lite sobre la misma foto para comparar los bordes y el tiempo de procesamiento. Desconectá la red y repetí con un modelo instalado.
