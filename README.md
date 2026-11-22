# Recorte

MVP de escritorio para quitar el fondo de una imagen en Windows x64. Acepta PNG, JPEG y WebP, muestra el antes y el después y exporta PNG con transparencia a la resolución de la imagen original. Se procesa una imagen por vez.

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
- **Recomendación** por RAM total y núcleos lógicos accesibles. Sugiere el modelo completo desde 12 GiB y 8 núcleos lógicos. Es una heurística inicial sin mediciones de rendimiento. La detección de GPU y la aceleración quedan fuera del MVP.
- **Modelos** [U²-Net](https://github.com/xuebinqin/U-2-Net) y U²-Net ligero, con exportaciones ONNX de [rembg](https://github.com/danielgatis/rembg/releases/tag/v0.0.0). Después de instalar un modelo, el flujo de abrir, procesar y exportar funciona localmente. La descarga de modelos es la única operación de la aplicación que hace peticiones de red.

## Verificación de escritorio

Abrí una foto, descargá U²-Net ligero y compará las dos vistas. Exportá el PNG y revisá dimensiones y canal alfa en un editor de imágenes. Desconectá la red y repetí con otra foto. Reconectá para instalar el segundo modelo y verificá que se pueda alternar y eliminar cada uno. Medí el consumo con la aplicación en reposo y durante el recorte antes de fijar un presupuesto de RAM.
